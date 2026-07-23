#![allow(clippy::unwrap_used, reason = "isolated temporary startup fixtures")]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

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
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
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
        SlotView::new(
            SlotId::parse("preview").unwrap(),
            DataInodes::new(201, 202).unwrap(),
        ),
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
fn unattributed_ordinary_journal_corruption_is_not_assigned_to_an_arbitrary_package() {
    let (root, roots, _key) = fixture();
    let arbitrary = PackageKey::new(
        PackageName::parse("com.example.arbitrary").unwrap(),
        UserId::PRIMARY,
    );
    let journal_root = root.path().join("journal");
    JournalStore::new(&journal_root).unwrap();
    fs::create_dir(journal_root.join("transactions/corrupt-published")).unwrap();
    fs::create_dir_all(
        roots
            .enrollment
            .join("packages")
            .join(arbitrary.package_name().as_str()),
    )
    .unwrap();

    assert!(management_artifacts_present(&roots, &arbitrary).is_err());
}

#[test]
fn weak_package_artifact_does_not_hide_global_journal_corruption() {
    let (root, roots, key) = fixture();
    let journal_root = root.path().join("journal");
    JournalStore::new(&journal_root).unwrap();
    fs::create_dir(journal_root.join("transactions/corrupt-published")).unwrap();
    fs::create_dir_all(roots.enrollment.join("packages").join(ALLOWED_PACKAGE)).unwrap();
    fs::write(
        roots
            .enrollment
            .join("packages")
            .join(ALLOWED_PACKAGE)
            .join("enrollment.json"),
        b"not-a-valid-enrollment",
    )
    .unwrap();

    assert!(management_artifacts_present(&roots, &key).is_err());
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
