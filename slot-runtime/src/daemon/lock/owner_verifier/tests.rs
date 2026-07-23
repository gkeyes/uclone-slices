#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    missing_docs,
    reason = "owner-verifier fixtures fail the individual test immediately"
)]

use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};

use tempfile::TempDir;

use super::*;
use crate::domain::BootId;
use crate::emergency_manifest::RuntimeOwnerRole;

fn proof(path: &std::path::Path, pid: u32, start_ticks: u64) -> RuntimeOwnerProof {
    let metadata = fs::metadata(path).expect("lock metadata");
    RuntimeOwnerProof::new(
        BootId::parse("host-test-boot").expect("boot id"),
        pid,
        start_ticks,
        RuntimeOwnerRole::Ucloned,
        metadata.dev(),
        metadata.ino(),
    )
}

fn private_temp() -> TempDir {
    let temp = TempDir::new().expect("temp dir");
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).expect("private temp mode");
    temp
}

fn lock_fixture() -> (TempDir, std::path::PathBuf) {
    let temp = private_temp();
    let path = temp.path().join("runtime.lock");
    fs::create_dir(&path).expect("lock dir");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("lock mode");
    (temp, path)
}

#[cfg(any(target_os = "android", target_os = "linux"))]
#[test]
fn linux_owner_path_is_pinned_to_open_directory_fd() {
    let (_temp, path) = lock_fixture();
    let pinned = super::lock_directory::PinnedLockDirectory::open(&path).unwrap();
    let owner_path = pinned.owner_path();
    assert!(owner_path.starts_with("/proc/self/fd/"));
    assert_eq!(owner_path.file_name(), Some(std::ffi::OsStr::new("owner")));
}

#[cfg(any(target_os = "android", target_os = "linux"))]
#[test]
fn replacing_lock_path_does_not_change_pinned_owner() {
    let (temp, path) = lock_fixture();
    let owner = path.join("owner");
    fs::write(
        &owner,
        b"pid=1\nboot=host-test-boot\nstart_ticks=1\nrole=ucloned\n",
    )
    .unwrap();
    fs::set_permissions(&owner, fs::Permissions::from_mode(0o600)).unwrap();
    let expected = fs::metadata(&path).unwrap();
    let pinned = super::lock_directory::PinnedLockDirectory::open(&path).unwrap();
    let old_path = temp.path().join("runtime.lock.old");
    fs::rename(&path, &old_path).unwrap();
    fs::create_dir(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    let replacement_owner = path.join("owner");
    fs::write(
        &replacement_owner,
        b"pid=2\nboot=host-test-boot\nstart_ticks=2\nrole=ucloned\n",
    )
    .unwrap();
    fs::set_permissions(&replacement_owner, fs::Permissions::from_mode(0o600)).unwrap();
    let pinned_owner = fs::read(pinned.owner_path()).unwrap();
    assert!(pinned_owner.starts_with(b"pid=1\n"));
    assert!(!pinned.same_after(&expected, expected.uid()).unwrap());
}

#[test]
fn missing_lock_is_a_non_mutating_absence() {
    let temp = private_temp();
    let path = temp.path().join("runtime.lock");
    let proof = RuntimeOwnerProof::new(
        BootId::parse("host-test-boot").unwrap(),
        1,
        1,
        RuntimeOwnerRole::Ucloned,
        1,
        1,
    );
    assert_eq!(
        verify_runtime_owner(&path, &proof).unwrap(),
        RuntimeOwnerVerdict::Missing
    );
    assert!(!path.exists());
}

#[test]
fn symlink_lock_is_rejected_without_following_it() {
    let temp = private_temp();
    let real = temp.path().join("real.lock");
    fs::create_dir(&real).unwrap();
    fs::set_permissions(&real, fs::Permissions::from_mode(0o700)).unwrap();
    let linked = temp.path().join("runtime.lock");
    symlink(&real, &linked).unwrap();
    let proof = proof(&real, 1, 1);
    assert_eq!(
        verify_runtime_owner(&linked, &proof).unwrap(),
        RuntimeOwnerVerdict::Invalid(RuntimeOwnerMismatch::LockSymlink)
    );
    assert!(real.is_dir());
}

#[test]
fn lock_mode_is_part_of_the_live_proof() {
    let (_temp, path) = lock_fixture();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    let proof = proof(&path, 1, 1);
    assert_eq!(
        verify_runtime_owner(&path, &proof).unwrap(),
        RuntimeOwnerVerdict::Invalid(RuntimeOwnerMismatch::LockMode)
    );
}

#[test]
fn malformed_owner_record_is_rejected() {
    let (_temp, path) = lock_fixture();
    let owner = path.join("owner");
    fs::write(&owner, b"not-an-owner\n").unwrap();
    fs::set_permissions(&owner, fs::Permissions::from_mode(0o600)).unwrap();
    let proof = proof(&path, 1, 1);
    assert_eq!(
        verify_runtime_owner(&path, &proof).unwrap(),
        RuntimeOwnerVerdict::Invalid(RuntimeOwnerMismatch::OwnerMalformed)
    );
}

#[test]
fn multiply_linked_owner_record_is_rejected() {
    let (_temp, path) = lock_fixture();
    let owner = path.join("owner");
    let alias = path.join("owner.alias");
    fs::write(
        &owner,
        b"pid=1\nboot=host-test-boot\nstart_ticks=1\nrole=ucloned\n",
    )
    .unwrap();
    fs::set_permissions(&owner, fs::Permissions::from_mode(0o600)).unwrap();
    fs::hard_link(&owner, &alias).unwrap();
    let proof = proof(&path, 1, 1);
    assert_eq!(
        verify_runtime_owner(&path, &proof).unwrap(),
        RuntimeOwnerVerdict::Invalid(RuntimeOwnerMismatch::OwnerMetadata)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn current_host_owner_is_valid_when_record_and_process_match() {
    let (_temp, path) = lock_fixture();
    let current = std::process::id();
    let proof = proof(&path, current, 1);
    fs::write(
        path.join("owner"),
        format!("pid={current}\nboot=host-test-boot\nstart_ticks=1\nrole=ucloned\n"),
    )
    .unwrap();
    fs::set_permissions(path.join("owner"), fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        verify_runtime_owner(&path, &proof).unwrap(),
        RuntimeOwnerVerdict::Valid
    );
}
