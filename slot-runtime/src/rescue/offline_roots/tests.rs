#![allow(clippy::unwrap_used, reason = "isolated temporary startup fixtures")]

use std::fs;

use tempfile::TempDir;

use super::*;
use crate::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageName,
    SlotId, SlotView, TransactionId,
};
use crate::journal::{JournalStore, TransactionSpec};
use crate::lifecycle::LifecycleState;
use crate::protocol::ALLOWED_PACKAGE;

fn fixture() -> (TempDir, RescueRoots, PackageKey) {
    let root = TempDir::new().unwrap();
    let roots = RescueRoots::with_roots(
        root.path().join("enrollment"),
        root.path().join("catalog"),
        root.path().join("rescue-journal"),
    );
    let key = PackageKey::new(
        PackageName::parse(ALLOWED_PACKAGE).unwrap(),
        UserId::PRIMARY,
    );
    (root, roots, key)
}

#[test]
fn empty_control_plane_is_not_managed() {
    let (_root, roots, key) = fixture();
    assert!(!management_artifacts_present(&roots, &key).unwrap());
}

#[test]
fn ordinary_transaction_journal_is_a_management_artifact() {
    let (root, roots, key) = fixture();
    let inodes = DataInodes::new(101, 102).unwrap();
    let identity =
        AppIdentity::new(10_321, &"ab".repeat(32), 7, "/data/app/test/base.apk").unwrap();
    let managed = ManagedPackage::new(
        key.clone(),
        identity,
        inodes,
        SlotView::new(SlotId::base(), inodes),
        LifecycleState::Normal,
    )
    .unwrap();
    let spec = TransactionSpec::new(
        TransactionId::parse("orphan-prepared").unwrap(),
        managed,
        SlotView::new(SlotId::base(), inodes),
        GateSnapshot::new(PackageEnabledState::Default, false),
        "boot-journal-only",
    )
    .unwrap();
    JournalStore::new(root.path().join("journal"))
        .unwrap()
        .create(&spec)
        .unwrap();

    assert!(management_artifacts_present(&roots, &key).unwrap());
}

#[test]
fn enrollment_attempt_uses_the_real_attempts_directory() {
    let (root, roots, key) = fixture();
    fs::create_dir_all(
        root.path()
            .join("enrollment-attempts/attempts")
            .join(ALLOWED_PACKAGE),
    )
    .unwrap();
    assert!(management_artifacts_present(&roots, &key).unwrap());
}

#[test]
fn malformed_enrollment_package_directory_is_still_managed() {
    let (root, roots, key) = fixture();
    fs::create_dir_all(
        root.path()
            .join("enrollment/packages")
            .join(ALLOWED_PACKAGE),
    )
    .unwrap();
    assert!(management_artifacts_present(&roots, &key).unwrap());
}

#[test]
fn rescue_journal_epoch_is_a_management_artifact() {
    let (root, roots, key) = fixture();
    fs::create_dir_all(
        root.path()
            .join("rescue-journal/packages")
            .join(ALLOWED_PACKAGE)
            .join("rescue/steps"),
    )
    .unwrap();
    assert!(management_artifacts_present(&roots, &key).unwrap());
}

#[test]
fn compatibility_policy_orphan_is_still_managed() {
    let (root, roots, key) = fixture();
    fs::create_dir_all(
        root.path()
            .join("compatibility-policy/packages")
            .join(ALLOWED_PACKAGE),
    )
    .unwrap();
    assert!(management_artifacts_present(&roots, &key).unwrap());
}

#[test]
fn preliminary_gate_lease_is_a_management_artifact() {
    let (root, roots, key) = fixture();
    let state = root.path().join("state");
    fs::create_dir(&state).unwrap();
    fs::write(state.join(format!("{ALLOWED_PACKAGE}.gate")), b"prepared").unwrap();
    assert!(management_artifacts_present(&roots, &key).unwrap());
}

#[test]
fn retiring_gate_lease_is_a_management_artifact() {
    let (root, roots, key) = fixture();
    let state = root.path().join("state");
    fs::create_dir(&state).unwrap();
    fs::write(
        state.join(format!(".{ALLOWED_PACKAGE}.gate.retiring")),
        b"retiring",
    )
    .unwrap();
    assert!(management_artifacts_present(&roots, &key).unwrap());
}

#[test]
fn retired_gate_evidence_is_a_management_artifact() {
    let (root, roots, key) = fixture();
    let state = root.path().join("state");
    fs::create_dir(&state).unwrap();
    fs::write(
        state.join(format!(".{ALLOWED_PACKAGE}.gate.retired")),
        b"retired",
    )
    .unwrap();
    assert!(management_artifacts_present(&roots, &key).unwrap());
}
