use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

mod owner_verifier;
mod platform;
mod record;

pub use owner_verifier::{
    RuntimeOwnerMismatch, RuntimeOwnerVerdict, RuntimeOwnerVerificationError, verify_runtime_owner,
};

use record::OwnerRecord;

/// Bytes accepted for the identity files in the lock artifact.
pub(super) const MAX_OWNER_BYTES: u64 = 512;
const INITIALIZATION_GRACE: Duration = Duration::from_secs(5);
const OWNER_FILE: &str = "owner";
const INITIALIZING_FILE: &str = "initializing";
static IDENTITY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Failure to acquire or verify the sole fixed runtime lock.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeLockError {
    /// Another live daemon or direct rescue client owns the lock.
    #[error("runtime lock is busy")]
    Busy,
    /// The lock artifact or requested role violates the fixed contract.
    #[error("runtime lock artifact is invalid")]
    Invalid,
    /// Lock publication or liveness verification failed.
    #[error("runtime lock I/O: {0}")]
    Io(#[from] io::Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExistingLock {
    Live,
    Stale,
}

/// Owned lease for the sole daemon/direct-rescue mutation lock.
#[derive(Debug)]
pub struct RuntimeLock {
    path: PathBuf,
    owner: OwnerRecord,
}

impl RuntimeLock {
    /// Acquires the sole fixed runtime mutation lock for the daemon or rescue client.
    pub fn acquire(path: &Path, role: &str) -> Result<Self, RuntimeLockError> {
        if !matches!(role, "ucloned" | "slotctl") {
            return Err(RuntimeLockError::Invalid);
        }
        let parent = path.parent().ok_or(RuntimeLockError::Invalid)?;
        let parent_metadata = fs::symlink_metadata(parent)?;
        if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
            return Err(RuntimeLockError::Invalid);
        }
        let owner = OwnerRecord::current(role)?;
        for attempt in 0..3 {
            match fs::create_dir(path) {
                Ok(()) => return Self::publish(path, owner),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    if path_is_symlink(path)? {
                        return Err(RuntimeLockError::Invalid);
                    }
                    match Self::inspect_existing(path)? {
                        ExistingLock::Live => return Err(RuntimeLockError::Busy),
                        ExistingLock::Stale => match Self::quarantine_stale(path) {
                            Ok(()) => {}
                            Err(RuntimeLockError::Io(rename_error))
                                if rename_error.kind() == io::ErrorKind::NotFound
                                    && attempt < 2 => {}
                            Err(error) => return Err(error),
                        },
                    }
                }
                Err(error) => return Err(RuntimeLockError::Io(error)),
            }
        }
        Err(RuntimeLockError::Busy)
    }

    fn publish(path: &Path, owner: OwnerRecord) -> Result<Self, RuntimeLockError> {
        let result = Self::publish_files(path, &owner);
        if let Err(error) = result {
            let _ = fs::remove_file(path.join(OWNER_FILE));
            let _ = fs::remove_file(path.join(INITIALIZING_FILE));
            let _ = fs::remove_dir(path);
            return Err(error);
        }
        Ok(Self {
            path: path.to_path_buf(),
            owner,
        })
    }

    fn publish_files(path: &Path, owner: &OwnerRecord) -> Result<(), RuntimeLockError> {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        write_identity_file(&path.join(INITIALIZING_FILE), owner)?;
        write_identity_file(&path.join(OWNER_FILE), owner)?;
        Ok(())
    }

    fn inspect_existing(path: &Path) -> Result<ExistingLock, RuntimeLockError> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() {
            return Err(RuntimeLockError::Invalid);
        }
        let owner_path = path.join(OWNER_FILE);
        match fs::symlink_metadata(&owner_path) {
            Ok(owner_metadata) => {
                if owner_metadata.file_type().is_symlink() {
                    return Err(RuntimeLockError::Invalid);
                }
                let owner = OwnerRecord::read(&owner_path)?;
                return Ok(if owner.is_live()? {
                    ExistingLock::Live
                } else {
                    ExistingLock::Stale
                });
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(RuntimeLockError::Io(error)),
        }
        let initializing = path.join(INITIALIZING_FILE);
        match fs::symlink_metadata(&initializing) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(RuntimeLockError::Invalid);
                }
                let record = OwnerRecord::read(&initializing)?;
                Ok(if record.is_live()? {
                    ExistingLock::Live
                } else {
                    ExistingLock::Stale
                })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                ownerless_state(metadata.modified()?)
            }
            Err(error) => Err(RuntimeLockError::Io(error)),
        }
    }

    fn quarantine_stale(path: &Path) -> Result<(), RuntimeLockError> {
        let parent = path.parent().ok_or(RuntimeLockError::Invalid)?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or(RuntimeLockError::Invalid)?;
        let orphan = parent.join(format!(".{name}.stale-{}", std::process::id()));
        if fs::symlink_metadata(&orphan).is_ok() {
            return Err(RuntimeLockError::Busy);
        }
        fs::rename(path, orphan)?;
        sync_directory(parent)?;
        Ok(())
    }
}

impl Drop for RuntimeLock {
    fn drop(&mut self) {
        let owner_path = self.path.join(OWNER_FILE);
        let matches = OwnerRecord::read(&owner_path).is_ok_and(|owner| owner == self.owner);
        if !matches {
            return;
        }
        let _ = fs::remove_file(owner_path);
        let _ = fs::remove_file(self.path.join(INITIALIZING_FILE));
        let _ = fs::remove_dir(&self.path);
    }
}

fn write_identity_file(path: &Path, owner: &OwnerRecord) -> Result<(), RuntimeLockError> {
    let parent = path.parent().ok_or(RuntimeLockError::Invalid)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or(RuntimeLockError::Invalid)?;
    let sequence = IDENTITY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(".{name}.tmp-{}-{sequence}", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(owner.body().as_bytes())?;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        sync_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(RuntimeLockError::Io)
}

fn path_is_symlink(path: &Path) -> Result<bool, RuntimeLockError> {
    Ok(fs::symlink_metadata(path)?.file_type().is_symlink())
}

fn ownerless_state(modified: SystemTime) -> Result<ExistingLock, RuntimeLockError> {
    let age = SystemTime::now()
        .duration_since(modified)
        .map_err(|_| RuntimeLockError::Invalid)?;
    if age <= INITIALIZATION_GRACE {
        return Ok(ExistingLock::Live);
    }
    Ok(ExistingLock::Stale)
}

fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}
