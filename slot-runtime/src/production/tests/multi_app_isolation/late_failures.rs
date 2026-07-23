use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::super::super::composition::ProductionPlatform;
use super::super::super::metadata::SystemMetadataSource;
use super::super::orphan_gate::{probe, stores};
use super::super::slot_lifecycle_support::TrackingMaterializer;
use super::runtime::MultiPackageRuntime;
use super::{publish, request};
use crate::domain::{PackageKey, PackageName};
use crate::package_state::PackageStateStore;
use crate::protocol::{Command, ErrorCode, ResponsePayload};
use crate::reconcile::ReconcileOutcome;
use crate::service::{PreviewService, ServiceError};

#[test]
fn late_store_failure_reacquires_a_retired_gate_before_state_marking() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let stores = stores(root.path());
    let package = PackageName::parse("com.example.late").unwrap();
    let managed = publish(&stores, package.clone());
    let revisions = root
        .join("package-state/packages")
        .join(package.as_str())
        .join("revisions");
    fs::remove_dir_all(&revisions).unwrap();
    fs::write(&revisions, b"corrupt-state").unwrap();
    let mut platform = ProductionPlatform {
        runtime: MultiPackageRuntime::new(),
        materializer: TrackingMaterializer::healthy(),
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: SystemMetadataSource::new(),
        stores,
        recovery_overrides: Default::default(),
    };
    let key = PackageKey::new(managed.package_name().clone(), managed.user_id());

    assert!(!platform.runtime().held(&package));
    assert!(platform.contain_reconcile_failure(&key, ServiceError::RecoveryRequired));
    assert!(platform.runtime().held(&package));

    let mut service = PreviewService::new(platform);
    assert_eq!(
        service
            .handle(&request(
                "late-state",
                Command::StatusPackage {
                    package: package.clone(),
                },
            ))
            .error_code(),
        Some(ErrorCode::RecoveryRequired)
    );
    assert_eq!(
        service
            .handle(&request("late-slots", Command::ListSlots { package },))
            .error_code(),
        Some(ErrorCode::RecoveryRequired)
    );
}

#[test]
fn late_attempt_failure_contains_one_package_and_healthy_peer_can_reconcile() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let stores = stores(root.path());
    let package_a = PackageName::parse("com.example.latebroken").unwrap();
    let package_b = PackageName::parse("com.example.latehealthy").unwrap();
    let managed_a = publish(&stores, package_a.clone());
    let managed_b = publish(&stores, package_b.clone());
    let corrupt_attempt = root
        .join("enrollment-attempts/attempts")
        .join(package_a.as_str());
    fs::write(&corrupt_attempt, b"corrupt-attempt").unwrap();
    let mut platform = ProductionPlatform {
        runtime: MultiPackageRuntime::new(),
        materializer: TrackingMaterializer::healthy(),
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: SystemMetadataSource::new(),
        stores,
        recovery_overrides: Default::default(),
    };
    let key_a = PackageKey::new(managed_a.package_name().clone(), managed_a.user_id());
    let key_b = PackageKey::new(managed_b.package_name().clone(), managed_b.user_id());

    assert!(matches!(
        platform.reconcile_ordinary(&key_a).unwrap(),
        ReconcileOutcome::RecoveryRequired(_)
    ));
    assert!(platform.runtime().held(&package_a));
    assert!(matches!(
        platform.reconcile_ordinary(&key_b).unwrap(),
        ReconcileOutcome::RestoredBase
    ));
    assert!(platform.runtime().held(&package_a));
    assert!(!platform.runtime().held(&package_b));

    let mut service = PreviewService::new(platform);
    assert_eq!(
        service
            .handle(&request(
                "late-broken-status",
                Command::StatusPackage {
                    package: package_a.clone(),
                },
            ))
            .error_code(),
        Some(ErrorCode::RecoveryRequired)
    );
    assert!(matches!(
        service
            .handle(&request(
                "late-healthy-status",
                Command::StatusPackage { package: package_b },
            ))
            .payload(),
        Some(ResponsePayload::PackageStatus(value))
            if value.package() == &PackageName::parse("com.example.latehealthy").unwrap()
    ));
}

#[test]
fn corrupt_package_state_revision_isolated_from_healthy_peer_and_runtime_reopen() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let mut stores = stores(root.path());
    let package_a = PackageName::parse("com.example.statebroken").unwrap();
    let package_b = PackageName::parse("com.example.statehealthy").unwrap();
    let managed_a = publish(&stores, package_a.clone());
    let managed_b = publish(&stores, package_b.clone());
    fs::write(
        stores
            .package_state
            .revision_path(managed_a.package_name(), 1),
        b"corrupt-state",
    )
    .unwrap();
    stores.package_state = PackageStateStore::new(root.path().join("package-state")).unwrap();
    let platform = ProductionPlatform {
        runtime: MultiPackageRuntime::new(),
        materializer: TrackingMaterializer::healthy(),
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: SystemMetadataSource::new(),
        stores,
        recovery_overrides: Default::default(),
    };
    let mut service = PreviewService::new(platform);

    let apps = service.handle(&request("state-apps", Command::ListManagedApps));
    let Some(ResponsePayload::ManagedApps(report)) = apps.payload() else {
        panic!("missing managed-app payload");
    };
    assert!(report.apps().iter().any(|row| {
        row.package() == &package_a
            && row.lifecycle() == crate::lifecycle::LifecycleState::RecoveryRequired
    }));
    assert!(report.apps().iter().any(|row| {
        row.package() == &package_b && row.lifecycle() == crate::lifecycle::LifecycleState::Normal
    }));
    assert_eq!(
        service
            .handle(&request(
                "state-broken-status",
                Command::StatusPackage { package: package_a },
            ))
            .error_code(),
        Some(ErrorCode::RecoveryRequired)
    );
    assert!(matches!(
        service
            .handle(&request(
                "state-healthy-status",
                Command::StatusPackage { package: package_b },
            ))
            .payload(),
        Some(ResponsePayload::PackageStatus(value))
            if value.package() == managed_b.package_name()
    ));
}
