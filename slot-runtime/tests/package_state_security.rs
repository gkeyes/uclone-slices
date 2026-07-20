#![doc = "Adversarial filesystem-boundary tests for package lifecycle state."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test"
)]

use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};

use tempfile::TempDir;
use uclone_slot_runtime::domain::{PackageKey, PackageName, UserId};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::package_state::{PackageStateReason, PackageStateStore};

fn key(name: &str) -> PackageKey {
    PackageKey::new(PackageName::parse(name).unwrap(), UserId::PRIMARY)
}

fn secure_temp_dir() -> TempDir {
    let root = TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    root
}

#[test]
fn rejects_symlinked_root_and_packages_without_following_targets() {
    let parent = secure_temp_dir();
    let outside = TempDir::new().unwrap();
    let root_link = parent.path().join("state");
    symlink(outside.path(), &root_link).unwrap();

    assert!(PackageStateStore::new(&root_link).is_err());
    assert!(!outside.path().join("packages").exists());

    let root = parent.path().join("real-state");
    PackageStateStore::new(&root).unwrap();
    let packages = root.join("packages");
    fs::remove_dir(&packages).unwrap();
    symlink(outside.path(), &packages).unwrap();

    assert!(PackageStateStore::new(&root).is_err());
    assert!(!outside.path().join("packages").exists());
}

#[test]
fn rejects_symlinked_package_and_revision_directories() {
    let root = secure_temp_dir();
    let outside = TempDir::new().unwrap();
    let package = key("com.uclone.symlink");
    let state = PackageStateStore::new(root.path()).unwrap();
    state.initialize(&package).unwrap();

    let package_directory = root.path().join("packages/com.uclone.symlink");
    let moved_package = outside.path().join("package");
    fs::rename(&package_directory, &moved_package).unwrap();
    symlink(&moved_package, &package_directory).unwrap();
    assert!(state.latest(&package).is_err());
    fs::remove_file(&package_directory).unwrap();
    fs::rename(moved_package, &package_directory).unwrap();

    let revisions = package_directory.join("revisions");
    let moved_revisions = outside.path().join("revisions");
    fs::rename(&revisions, &moved_revisions).unwrap();
    symlink(&moved_revisions, &revisions).unwrap();
    assert!(state.latest(&package).is_err());
}

#[test]
fn rejects_a_hard_linked_revision_record() {
    let root = secure_temp_dir();
    let package = key("com.uclone.hardlink");
    let state = PackageStateStore::new(root.path()).unwrap();
    state.initialize(&package).unwrap();

    let revision = state.revision_path(package.package_name(), 1);
    let alias = state.revision_path(package.package_name(), 2);
    fs::hard_link(&revision, &alias).unwrap();

    assert!(state.latest(&package).is_err());
}

fn hidden_latest_revision_is_rejected(
    package_name: &str,
    next: LifecycleState,
    reason: PackageStateReason,
) {
    let root = secure_temp_dir();
    let package = key(package_name);
    let state = PackageStateStore::new(root.path()).unwrap();
    state.initialize(&package).unwrap();
    state
        .transition(&package, LifecycleState::Normal, next, reason)
        .unwrap();
    let latest = state.revision_path(package.package_name(), 2);
    let hidden = latest.with_file_name(".0000000000000002.json");
    fs::rename(latest, hidden).unwrap();

    assert!(state.latest(&package).is_err());
}

#[test]
fn hidden_latest_recovery_and_quarantine_states_cannot_roll_back() {
    hidden_latest_revision_is_rejected(
        "com.uclone.hidden.recovery",
        LifecycleState::RecoveryRequired,
        PackageStateReason::ViewUncertain,
    );
    hidden_latest_revision_is_rejected(
        "com.uclone.hidden.quarantine",
        LifecycleState::Quarantined,
        PackageStateReason::IdentityChanged,
    );
}
