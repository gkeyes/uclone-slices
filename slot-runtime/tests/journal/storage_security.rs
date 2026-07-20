use super::spec;

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{PermissionsExt as _, symlink};

use tempfile::TempDir;
use uclone_slot_runtime::journal::{JournalError, JournalStore};

const OVERSIZED_PADDING: usize = 128 * 1024;

#[test]
fn rejects_oversized_digest_valid_step() {
    let root = TempDir::new().unwrap();
    super::support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();
    let path = store.step_path(spec.transaction_id(), 1).unwrap();
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    let padding = vec![b' '; OVERSIZED_PADDING];
    file.write_all(&padding).unwrap();

    let error = store.load(spec.transaction_id()).unwrap_err();

    assert!(matches!(error, JournalError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted journal step"));
}

#[test]
fn rejects_step_with_permissive_mode() {
    let root = TempDir::new().unwrap();
    super::support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();
    let path = store.step_path(spec.transaction_id(), 1).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();

    let error = store.load(spec.transaction_id()).unwrap_err();

    assert!(matches!(error, JournalError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted journal step"));
}

#[test]
fn rejects_hard_linked_step() {
    let root = TempDir::new().unwrap();
    super::support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();
    let path = store.step_path(spec.transaction_id(), 1).unwrap();
    fs::hard_link(&path, root.path().join("external-step-link")).unwrap();

    let error = store.load(spec.transaction_id()).unwrap_err();

    assert!(matches!(error, JournalError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted journal step"));
}

#[test]
fn rejects_symlinked_step_without_following_it() {
    let root = TempDir::new().unwrap();
    super::support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();
    let path = store.step_path(spec.transaction_id(), 1).unwrap();
    let target = root.path().join("saved-step.json");
    fs::rename(&path, &target).unwrap();
    symlink(target, path).unwrap();

    let error = store.load(spec.transaction_id()).unwrap_err();

    assert!(matches!(error, JournalError::Corrupt(_)));
}

#[test]
fn rejects_steps_directory_with_permissive_mode() {
    let root = TempDir::new().unwrap();
    super::support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = spec();
    store.create(&spec).unwrap();
    let steps = store
        .transaction_path(spec.transaction_id())
        .unwrap()
        .join("steps");
    fs::set_permissions(steps, fs::Permissions::from_mode(0o755)).unwrap();

    let error = store.load(spec.transaction_id()).unwrap_err();

    assert!(matches!(error, JournalError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted journal directory"));
}

#[test]
fn rejects_existing_journal_root_with_permissive_mode() {
    let root = TempDir::new().unwrap();
    super::support::secure_temp_dir(&root);
    let journal = root.path().join("journal");
    fs::create_dir(&journal).unwrap();
    fs::set_permissions(&journal, fs::Permissions::from_mode(0o755)).unwrap();

    let error = JournalStore::new(journal).unwrap_err();

    assert!(matches!(error, JournalError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted journal directory"));
}

#[test]
fn rejects_symlinked_journal_parent() {
    let root = TempDir::new().unwrap();
    super::support::secure_temp_dir(&root);
    let target = root.path().join("real-parent");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
    let linked_parent = root.path().join("linked-parent");
    symlink(target, &linked_parent).unwrap();

    let error = JournalStore::new(linked_parent.join("journal")).unwrap_err();

    assert!(matches!(error, JournalError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted journal directory"));
}

#[test]
fn rejects_relative_journal_root() {
    let error = JournalStore::new("relative-journal-root").unwrap_err();

    assert!(matches!(error, JournalError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted journal root path"));
}
