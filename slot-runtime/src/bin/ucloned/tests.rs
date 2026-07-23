#![allow(
    clippy::unwrap_used,
    reason = "daemon startup tests use isolated temporary directories and fail-fast fixtures"
)]

use std::collections::BTreeMap;
use std::fs;

use clap::Parser;
use tempfile::tempdir;

use super::discovery::{management_package_roots, reconciliation_keys, scan_package_root};
use super::{
    Cli, StartupPackageOutcome, UclonedError, reconcile_startup_result, record_startup_outcome,
    startup_gate_value,
};
use uclone_slot_runtime::domain::{PackageKey, PackageName, UserId};
use uclone_slot_runtime::reconcile::{ReconcileOutcome, ReconcileReason};
use uclone_slot_runtime::service::ServiceError;

#[test]
fn accepts_only_the_fixed_startup_gate_flag() {
    assert!(Cli::try_parse_from(["ucloned", "--startup-gate"]).is_ok());
    assert!(Cli::try_parse_from(["ucloned", "--package", "other"]).is_err());
}

#[test]
fn raw_management_scan_retains_recognizable_orphan_package() {
    let root = tempdir().unwrap();
    let package = root.path().join("com.xingin.xhs");
    fs::create_dir(&package).unwrap();
    fs::write(root.path().join("not-a-package"), b"corrupt").unwrap();
    let mut packages = BTreeMap::new();

    let corrupt = scan_package_root(root.path(), &mut packages);

    assert!(corrupt);
    assert!(packages.contains_key("com.xingin.xhs"));
}

#[test]
fn recognizable_package_artifact_is_attributed_instead_of_global() {
    let root = tempdir().unwrap();
    fs::write(root.path().join("com.example.broken"), b"not a directory").unwrap();
    let mut packages = BTreeMap::new();

    let corrupt = scan_package_root(root.path(), &mut packages);

    assert!(!corrupt);
    assert!(packages.contains_key("com.example.broken"));
}

#[test]
fn one_held_package_never_masks_another_corrupt_management_artifact() {
    assert!(startup_gate_value(1, true).is_err());
    assert_eq!(startup_gate_value(1, false).unwrap(), "held");
}

#[test]
fn early_management_discovery_includes_device_encrypted_slots() {
    assert!(
        management_package_roots()
            .iter()
            .any(|root| root == std::path::Path::new(uclone_slot_runtime::target::DE_SLOT_ROOT))
    );
}

#[test]
fn package_recovery_does_not_drop_healthy_startup_peer() {
    let broken = PackageKey::new(
        PackageName::parse("com.example.broken").unwrap(),
        UserId::PRIMARY,
    );
    let healthy = PackageKey::new(
        PackageName::parse("com.example.healthy").unwrap(),
        UserId::PRIMARY,
    );
    let mut ordinary = Vec::new();
    let mut recovery = Default::default();
    let mut retired = Default::default();

    record_startup_outcome(
        &broken,
        StartupPackageOutcome::RecoveryRequired,
        &mut ordinary,
        &mut recovery,
        &mut retired,
    );
    record_startup_outcome(
        &healthy,
        StartupPackageOutcome::Ordinary,
        &mut ordinary,
        &mut recovery,
        &mut retired,
    );

    assert_eq!(ordinary, [healthy.clone()]);
    assert!(recovery.contains(broken.package_name()));
    assert!(retired.is_empty());
    assert_eq!(reconciliation_keys(&ordinary, &recovery), [broken, healthy]);
}

#[test]
fn contained_recovery_continues_but_uncontained_error_fails_closed() {
    let package = PackageName::parse("com.example.startup").unwrap();
    let contained = reconcile_startup_result(
        &PackageKey::new(package.clone(), UserId::PRIMARY),
        Ok(ReconcileOutcome::RecoveryRequired(
            ReconcileReason::JournalMetadata,
        )),
    );
    assert!(contained.is_ok());

    let uncontained = reconcile_startup_result(
        &PackageKey::new(package, UserId::PRIMARY),
        Err(ServiceError::Internal),
    );
    assert!(matches!(
        uncontained,
        Err(UclonedError::Reconcile(message)) if message.contains("containment failed")
    ));
}
