#![doc = "Durable package lifecycle-state stream integrity tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test"
)]

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

use tempfile::TempDir;
use uclone_slot_runtime::domain::{PackageKey, PackageName, UserId};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::package_state::{PackageStateReason, PackageStateStore};

fn key(name: &str) -> PackageKey {
    PackageKey::new(PackageName::parse(name).unwrap(), UserId::PRIMARY)
}

fn store() -> (TempDir, PackageStateStore) {
    let root = TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let state = PackageStateStore::new(root.path()).unwrap();
    (root, state)
}

#[test]
fn initializes_and_hash_links_lifecycle_revisions() {
    let (_root, state) = store();
    let package = key("com.uclone.state");

    let first = state.initialize(&package).unwrap();
    let second = state
        .transition(
            &package,
            LifecycleState::Normal,
            LifecycleState::RecoveryRequired,
            PackageStateReason::ViewUncertain,
        )
        .unwrap();

    assert_eq!(first.generation(), 1);
    assert_eq!(first.lifecycle_state(), LifecycleState::Normal);
    assert_eq!(first.reason(), PackageStateReason::Enrolled);
    assert_eq!(first.previous_sha256(), None);
    assert_eq!(second.generation(), 2);
    assert_eq!(second.previous_sha256(), Some(first.sha256()));
    assert_eq!(state.latest(&package).unwrap(), Some(second));
}

#[test]
fn identity_change_is_quarantined_and_quarantine_is_terminal() {
    let (_root, state) = store();
    let package = key("com.uclone.quarantine");
    state.initialize(&package).unwrap();

    state
        .transition(
            &package,
            LifecycleState::Normal,
            LifecycleState::Quarantined,
            PackageStateReason::IdentityChanged,
        )
        .unwrap();
    let error = state
        .transition(
            &package,
            LifecycleState::Quarantined,
            LifecycleState::Normal,
            PackageStateReason::ManualRepair,
        )
        .unwrap_err();

    assert!(error.to_string().contains("illegal lifecycle transition"));
    assert_eq!(state.latest(&package).unwrap().unwrap().generation(), 2);
}

#[test]
fn rejects_wrong_expected_state_and_duplicate_initialization() {
    let (_root, state) = store();
    let package = key("com.uclone.expected");
    state.initialize(&package).unwrap();

    let wrong = state
        .transition(
            &package,
            LifecycleState::UpdatePreparing,
            LifecycleState::UpdateWindowOpen,
            PackageStateReason::LifecycleDrift,
        )
        .unwrap_err();
    assert!(wrong.to_string().contains("unexpected previous"));

    let duplicate = state.initialize(&package).unwrap_err();
    assert!(duplicate.to_string().contains("already exists"));
}

#[test]
fn generic_writer_refuses_managed_update_progression() {
    let (_root, state) = store();
    let package = key("com.uclone.update");
    state.initialize(&package).unwrap();
    let error = state
        .transition(
            &package,
            LifecycleState::Normal,
            LifecycleState::UpdatePreparing,
            PackageStateReason::ManagedUpdate,
        )
        .unwrap_err();

    assert!(error.to_string().contains("proof-bearing coordinator"));
    assert_eq!(state.latest(&package).unwrap().unwrap().generation(), 1);
}

#[test]
fn enumerates_sorted_packages_and_rejects_unexpected_artifacts() {
    let (root, state) = store();
    let first = key("com.uclone.zed");
    let second = key("com.uclone.alpha");
    state.initialize(&first).unwrap();
    state.initialize(&second).unwrap();

    let packages = state.enumerate_packages().unwrap();
    assert_eq!(
        packages,
        vec![second.package_name().clone(), first.package_name().clone()]
    );

    fs::write(root.path().join("packages/unexpected"), b"artifact").unwrap();
    let error = state.enumerate_packages().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unexpected package state artifact")
    );
}

#[test]
fn corrupt_stream_isolated_from_reopen_and_sibling_operations() {
    let (root, state) = store();
    let broken = key("com.uclone.broken");
    let healthy = key("com.uclone.healthy");
    let later = key("com.uclone.later");
    state.initialize(&broken).unwrap();
    state.initialize(&healthy).unwrap();
    fs::write(
        state.revision_path(broken.package_name(), 1),
        b"corrupt-state",
    )
    .unwrap();

    let reopened = PackageStateStore::new(root.path()).unwrap();
    assert!(reopened.latest(&broken).is_err());
    assert_eq!(
        reopened
            .latest(&healthy)
            .unwrap()
            .unwrap()
            .lifecycle_state(),
        LifecycleState::Normal
    );
    reopened
        .transition(
            &healthy,
            LifecycleState::Normal,
            LifecycleState::RecoveryRequired,
            PackageStateReason::ViewUncertain,
        )
        .unwrap();
    reopened.initialize(&later).unwrap();
    assert_eq!(
        reopened.enumerate_packages().unwrap(),
        vec![
            broken.package_name().clone(),
            healthy.package_name().clone(),
            later.package_name().clone(),
        ]
    );
}

#[test]
fn rejects_tampering_schema_and_filename_generation() {
    let (_root, state) = store();
    let package = key("com.uclone.tamper");
    state.initialize(&package).unwrap();
    let path = state.revision_path(package.package_name(), 1);
    let original = fs::read(&path).unwrap();

    let mut tampered = original.clone();
    let marker = tampered.iter().position(|byte| *byte == b'"').unwrap();
    *tampered.get_mut(marker).unwrap() = b'!';
    fs::write(&path, tampered).unwrap();
    assert!(
        state
            .latest(&package)
            .unwrap_err()
            .to_string()
            .contains("corrupt")
    );

    fs::write(&path, &original).unwrap();
    let mut schema = serde_json::from_slice::<serde_json::Value>(&original).unwrap();
    schema
        .as_object_mut()
        .unwrap()
        .insert("schema_version".to_owned(), serde_json::json!(99));
    fs::write(&path, serde_json::to_vec(&schema).unwrap()).unwrap();
    let error = state.latest(&package).unwrap_err();
    assert!(error.to_string().contains("schema version"));

    fs::write(&path, &original).unwrap();
    let renamed = state.revision_path(package.package_name(), 2);
    fs::rename(&path, &renamed).unwrap();
    let error = state.latest(&package).unwrap_err();
    assert!(error.to_string().contains("filename generation"));
}

#[test]
fn isolates_attributable_package_artifacts_but_rejects_root_artifacts() {
    let (root, state) = store();
    let package = key("com.uclone.dot");
    state.initialize(&package).unwrap();

    let root_artifact = root.path().join(".orphan");
    fs::write(&root_artifact, b"unexpected").unwrap();
    assert!(state.latest(&package).is_err());
    fs::remove_file(root_artifact).unwrap();

    let unattributed = root.path().join("packages/.orphan");
    fs::write(&unattributed, b"unexpected").unwrap();
    assert!(state.latest(&package).is_ok());
    assert!(state.enumerate_packages().is_err());
    fs::remove_file(unattributed).unwrap();

    let package_artifacts = [
        root.path().join("packages/com.uclone.dot/.orphan"),
        root.path()
            .join("packages/com.uclone.dot/revisions/.orphan"),
    ];
    for artifact in package_artifacts {
        fs::write(&artifact, b"unexpected").unwrap();
        let error = state.latest(&package).unwrap_err();
        assert!(error.to_string().contains("unexpected"));
        fs::remove_file(artifact).unwrap();
    }
}

#[test]
fn rejects_unsafe_state_directories_and_records() {
    let (root, state) = store();
    let package = key("com.uclone.permissions");
    state.initialize(&package).unwrap();
    let revisions = root
        .path()
        .join("packages/com.uclone.permissions/revisions");
    let path = state.revision_path(package.package_name(), 1);
    let original = fs::read(&path).unwrap();

    let package_directory = revisions.parent().unwrap();
    let directories = [
        root.path().to_path_buf(),
        root.path().join("packages"),
        package_directory.to_path_buf(),
        revisions.clone(),
    ];
    for directory in directories {
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(state.latest(&package).is_err());
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    }

    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(state.latest(&package).is_err());
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    fs::write(&path, vec![b'x'; 64 * 1024 + 1]).unwrap();
    assert!(state.latest(&package).is_err());
    fs::write(&path, original).unwrap();

    let backup = revisions.join("0000000000000002.json");
    fs::rename(&path, &backup).unwrap();
    symlink(&backup, &path).unwrap();
    assert!(state.latest(&package).is_err());
    fs::remove_file(&path).unwrap();
    fs::rename(backup, path).unwrap();
}
