use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};

use serde_json::Value;
use tempfile::TempDir;
use uclone_slot_runtime::domain::{DataInodes, SlotId, SlotView};
use uclone_slot_runtime::enrollment::EnrollmentStore;

#[test]
fn publishes_owner_only_enrollment_shape() {
    let root = super::secure_temp_dir();
    let store = EnrollmentStore::new(root.path()).unwrap();
    let enrolled = super::managed(SlotView::new(
        SlotId::base(),
        DataInodes::new(100, 200).unwrap(),
    ));
    store.create(&enrolled).unwrap();

    let packages = root.path().join("packages");
    let package = packages.join(enrolled.package_name().as_str());
    let record = package.join("enrollment.json");
    assert_eq!(
        fs::symlink_metadata(root.path()).unwrap().mode() & 0o7777,
        0o700
    );
    assert_eq!(
        fs::symlink_metadata(&packages).unwrap().mode() & 0o7777,
        0o700
    );
    assert_eq!(
        fs::symlink_metadata(&package).unwrap().mode() & 0o7777,
        0o700
    );
    assert_eq!(
        fs::symlink_metadata(&record).unwrap().mode() & 0o7777,
        0o600
    );
    assert_eq!(fs::symlink_metadata(&record).unwrap().nlink(), 1);
}

#[test]
fn rejects_oversized_enrollment_before_json_parsing() {
    let (root, store, enrolled) = published();
    let path = record_path(&root);
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(&vec![b' '; 128 * 1024]).unwrap();

    let error = store.load(enrolled.package_name()).unwrap_err();

    assert!(error.to_string().contains("untrusted enrollment record"));
}

#[test]
fn rejects_permissive_enrollment_record_mode() {
    let (root, store, enrolled) = published();
    fs::set_permissions(record_path(&root), fs::Permissions::from_mode(0o644)).unwrap();

    let error = store.load(enrolled.package_name()).unwrap_err();

    assert!(error.to_string().contains("untrusted enrollment record"));
}

#[test]
fn rejects_hard_linked_enrollment_record() {
    let (root, store, enrolled) = published();
    let path = record_path(&root);
    let outside = TempDir::new().unwrap();
    fs::hard_link(&path, outside.path().join("enrollment.json")).unwrap();

    let error = store.load(enrolled.package_name()).unwrap_err();

    assert!(error.to_string().contains("untrusted enrollment record"));
}

#[test]
fn rejects_symlinked_enrollment_record_without_following_target() {
    let (root, store, enrolled) = published();
    let path = record_path(&root);
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("enrollment.json");
    fs::rename(&path, &target).unwrap();
    symlink(&target, &path).unwrap();

    let error = store.load(enrolled.package_name()).unwrap_err();

    assert!(error.to_string().contains("unexpected enrollment artifact"));
    assert!(fs::symlink_metadata(&target).unwrap().file_type().is_file());
}

#[test]
fn rejects_non_regular_enrollment_record() {
    let (root, store, enrolled) = published();
    let path = record_path(&root);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();

    let error = store.load(enrolled.package_name()).unwrap_err();

    assert!(error.to_string().contains("unexpected enrollment artifact"));
}

#[test]
fn rejects_unknown_outer_and_nested_enrollment_fields() {
    let (root, store, enrolled) = published();
    let path = record_path(&root);
    let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();

    let mut outer = original.clone();
    outer
        .as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), Value::Bool(true));
    fs::write(&path, serde_json::to_vec(&outer).unwrap()).unwrap();
    assert!(
        store
            .load(enrolled.package_name())
            .unwrap_err()
            .to_string()
            .contains("unknown field")
    );

    let mut nested = original;
    nested
        .get_mut("managed")
        .and_then(|managed| managed.get_mut("identity"))
        .and_then(Value::as_object_mut)
        .unwrap()
        .insert("unexpected".to_owned(), Value::Bool(true));
    fs::write(&path, serde_json::to_vec(&nested).unwrap()).unwrap();
    assert!(
        store
            .load(enrolled.package_name())
            .unwrap_err()
            .to_string()
            .contains("unknown field")
    );
}

#[test]
fn rejects_unknown_nested_inode_field() {
    let (root, store, enrolled) = published();
    let path = record_path(&root);
    let mut wire: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    wire.get_mut("managed")
        .and_then(|managed| managed.get_mut("base_inodes"))
        .and_then(Value::as_object_mut)
        .unwrap()
        .insert("unexpected".to_owned(), Value::Bool(true));
    fs::write(&path, serde_json::to_vec(&wire).unwrap()).unwrap();

    let error = store.load(enrolled.package_name()).unwrap_err();

    assert!(error.to_string().contains("unknown field"));
}

#[test]
fn rejects_permissive_package_directory() {
    let (root, store, enrolled) = published();
    let package = root.path().join("packages/com.uclone.slotprobe");
    fs::set_permissions(package, fs::Permissions::from_mode(0o755)).unwrap();

    let error = store.load(enrolled.package_name()).unwrap_err();

    assert!(error.to_string().contains("untrusted enrollment package"));
}

#[test]
fn rejects_unknown_root_artifact() {
    let root = super::secure_temp_dir();
    let store = EnrollmentStore::new(root.path()).unwrap();
    fs::write(root.path().join("README"), b"unexpected").unwrap();

    let error = store.list().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("unexpected enrollment root artifact")
    );
}

fn published() -> (
    TempDir,
    EnrollmentStore,
    uclone_slot_runtime::domain::ManagedPackage,
) {
    let root = super::secure_temp_dir();
    let store = EnrollmentStore::new(root.path()).unwrap();
    let enrolled = super::managed(SlotView::new(
        SlotId::base(),
        DataInodes::new(100, 200).unwrap(),
    ));
    store.create(&enrolled).unwrap();
    (root, store, enrolled)
}

fn record_path(root: &TempDir) -> std::path::PathBuf {
    root.path()
        .join("packages/com.uclone.slotprobe/enrollment.json")
}
