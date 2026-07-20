#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    missing_docs,
    reason = "lock fixtures should fail the individual test immediately"
)]

use std::fs;

use tempfile::TempDir;
use uclone_slot_runtime::daemon::{RuntimeLock, RuntimeLockError};

#[test]
fn ownerless_runtime_lock_has_initialization_grace_before_reclamation() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("runtime.lock");
    fs::create_dir(&path).unwrap();
    assert!(matches!(
        RuntimeLock::acquire(&path, "slotctl"),
        Err(RuntimeLockError::Busy)
    ));
    assert!(path.exists());
}

#[test]
fn stale_initialization_identity_is_quarantined_and_lock_converges() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("runtime.lock");
    fs::create_dir(&path).unwrap();
    fs::write(
        path.join("initializing"),
        b"pid=4294967294\nboot=foreign-boot\nstart_ticks=7\nrole=slotctl\n",
    )
    .unwrap();

    let lock = RuntimeLock::acquire(&path, "slotctl").unwrap();
    assert!(path.join("owner").is_file());
    drop(lock);
    assert!(!path.exists());
    assert!(temp.path().read_dir().unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("stale-")
    }));
}

#[test]
fn malformed_initialization_identity_fails_closed_without_reclamation() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("runtime.lock");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("initializing"), b"unknown\n").unwrap();
    assert!(matches!(
        RuntimeLock::acquire(&path, "slotctl"),
        Err(RuntimeLockError::Invalid)
    ));
    assert!(path.exists());
}

#[test]
fn incomplete_temporary_identity_is_never_parsed_as_an_owner() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("runtime.lock");
    fs::create_dir(&path).unwrap();
    fs::write(path.join(".initializing.tmp-crashed"), b"pid=").unwrap();

    assert!(matches!(
        RuntimeLock::acquire(&path, "slotctl"),
        Err(RuntimeLockError::Busy)
    ));
    assert!(!path.join("owner").exists());
}

#[test]
fn lock_drop_does_not_remove_an_artifact_with_a_different_owner() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("runtime.lock");
    let lock = RuntimeLock::acquire(&path, "slotctl").unwrap();
    fs::write(
        path.join("owner"),
        b"pid=4294967294\nboot=foreign-boot\nstart_ticks=7\nrole=slotctl\n",
    )
    .unwrap();
    drop(lock);
    assert!(path.exists());
}

#[cfg(target_os = "macos")]
#[test]
fn live_initialization_identity_is_not_reclaimed() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("runtime.lock");
    fs::create_dir(&path).unwrap();
    fs::write(
        path.join("initializing"),
        format!(
            "pid={}\nboot=host-test-boot\nstart_ticks=1\nrole=slotctl\n",
            std::process::id()
        ),
    )
    .unwrap();
    assert!(matches!(
        RuntimeLock::acquire(&path, "ucloned"),
        Err(RuntimeLockError::Busy)
    ));
}
