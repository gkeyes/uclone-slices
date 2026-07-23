use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::orphan_gate::{probe, stores};
use super::slot_lifecycle_support::TrackingMaterializer;
use crate::daemon::RequestHandler;
use crate::domain::{ManagedPackage, PackageKey, PackageName, SlotId, SlotView, UserId};
use crate::lifecycle::LifecycleState;
use crate::production::composition::ProductionPlatform;
use crate::production::metadata::SystemMetadataSource;
use crate::protocol::{Command, ErrorCode, Request, RequestId, ResponsePayload, ResponseStatus};
use crate::reconcile::{ReconcileOutcome, RecoveryBackend};
use crate::service::PreviewService;

mod late_failures;
mod runtime;
use runtime::MultiPackageRuntime;

#[test]
fn broken_package_remains_contained_while_healthy_package_reconciles_and_serves() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let stores = stores(root.path());
    let package_a = PackageName::parse("com.example.broken").unwrap();
    let package_b = PackageName::parse("com.example.healthy").unwrap();
    let managed_a = publish(&stores, package_a.clone());
    let managed_b = publish(&stores, package_b.clone());
    fs::remove_dir_all(
        root.path()
            .join("enrollment/packages")
            .join(package_a.as_str()),
    )
    .unwrap();
    let mut runtime = MultiPackageRuntime::new();
    runtime.emergency_gate(&package_a).unwrap();
    let mut platform = ProductionPlatform {
        runtime,
        materializer: TrackingMaterializer::healthy(),
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: SystemMetadataSource::new(),
        stores,
        recovery_overrides: Default::default(),
    };
    let key_a = PackageKey::new(managed_a.package_name().clone(), managed_a.user_id());
    let key_b = PackageKey::new(managed_b.package_name().clone(), managed_b.user_id());

    assert_eq!(
        platform.do_reconcile(&key_a).unwrap(),
        ReconcileOutcome::RecoveryRequired(crate::reconcile::ReconcileReason::EnrollmentMetadata)
    );
    assert!(platform.runtime().held(&package_a));
    assert_eq!(
        platform.do_reconcile(&key_b).unwrap(),
        ReconcileOutcome::RestoredBase
    );
    assert!(platform.runtime().held(&package_a));
    assert!(!platform.runtime().held(&package_b));

    let mut service = PreviewService::new(platform);
    let apps = service.handle(&request("apps", Command::ListManagedApps));
    assert_eq!(apps.status(), ResponseStatus::Ok);
    let Some(ResponsePayload::ManagedApps(report)) = apps.payload() else {
        panic!("missing managed-app payload");
    };
    assert_eq!(report.apps().len(), 2);
    assert!(report.apps().iter().any(|row| {
        row.package() == &package_a && row.lifecycle() == LifecycleState::RecoveryRequired
    }));
    assert!(
        report.apps().iter().any(|row| {
            row.package() == &package_b && row.lifecycle() == LifecycleState::Normal
        })
    );

    for command in [
        Command::StatusPackage {
            package: package_a.clone(),
        },
        Command::ListSlots {
            package: package_a.clone(),
        },
    ] {
        assert_eq!(
            service.handle(&request("broken", command)).error_code(),
            Some(ErrorCode::RecoveryRequired)
        );
    }
    let status = service.handle(&request(
        "healthy-status",
        Command::StatusPackage {
            package: package_b.clone(),
        },
    ));
    assert!(matches!(
        status.payload(),
        Some(ResponsePayload::PackageStatus(value)) if value.package() == &package_b
    ));
    let slots = service.handle(&request(
        "healthy-slots",
        Command::ListSlots { package: package_b },
    ));
    assert!(matches!(
        slots.payload(),
        Some(ResponsePayload::Slots(value)) if value.slots().len() == 1
    ));
}

fn publish(
    stores: &super::super::stores::ProductionStores,
    package: PackageName,
) -> ManagedPackage {
    let key = PackageKey::new(package, UserId::PRIMARY);
    let base = probe::base_inodes();
    let managed = ManagedPackage::new(
        key.clone(),
        probe::identity(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap();
    stores.enrollment.create(&managed).unwrap();
    stores
        .compatibility_policy
        .create(
            key.package_name(),
            managed.identity(),
            crate::domain::PackageSupportLevel::Supported,
            false,
        )
        .unwrap();
    stores
        .catalog
        .create_base(
            key.clone(),
            base,
            managed.identity().clone(),
            probe::security_profile(),
        )
        .unwrap();
    stores.package_state.initialize(&key).unwrap();
    managed
}

fn request(id: &str, command: Command) -> Request {
    Request::new(RequestId::new(id).unwrap(), command).unwrap()
}
