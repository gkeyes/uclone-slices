use std::fs::{self, File};
use std::io::{self, Read};

use super::RuntimeLockError;
use super::platform::boot_id;
#[cfg(any(target_os = "android", target_os = "linux"))]
use super::platform::{process_role_matches, process_start_ticks};

/// The identity published before a runtime lock is considered live.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OwnerRecord {
    pub(super) pid: u32,
    pub(super) boot: String,
    pub(super) start_ticks: u64,
    pub(super) role: String,
}

impl OwnerRecord {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "Android/Linux identity reads are fallible; macOS host tests use fixed identity"
    )]
    pub(super) fn current(role: &str) -> Result<Self, RuntimeLockError> {
        let pid = std::process::id();
        #[cfg(any(target_os = "android", target_os = "linux"))]
        let boot = boot_id()?;
        #[cfg(target_os = "macos")]
        let boot = boot_id();
        #[cfg(any(target_os = "android", target_os = "linux"))]
        let start_ticks = process_start_ticks(pid)?.ok_or(RuntimeLockError::Invalid)?;
        #[cfg(target_os = "macos")]
        let start_ticks = 1_u64;
        Ok(Self {
            pid,
            boot,
            start_ticks,
            role: role.to_owned(),
        })
    }

    pub(super) fn read(path: &std::path::Path) -> Result<Self, RuntimeLockError> {
        let metadata = fs::symlink_metadata(path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                RuntimeLockError::Busy
            } else {
                RuntimeLockError::Io(error)
            }
        })?;
        if !metadata.file_type().is_file() || metadata.len() > super::MAX_OWNER_BYTES {
            return Err(RuntimeLockError::Invalid);
        }
        let mut bytes = Vec::new();
        File::open(path)?
            .take(super::MAX_OWNER_BYTES + 1)
            .read_to_end(&mut bytes)?;
        Self::parse(&bytes)
    }

    pub(super) fn parse(bytes: &[u8]) -> Result<Self, RuntimeLockError> {
        let text = core::str::from_utf8(bytes).map_err(|_| RuntimeLockError::Invalid)?;
        let fields: Vec<&str> = text.lines().collect();
        let [pid_field, boot_field, start_field, role_field] = fields.as_slice() else {
            return Err(RuntimeLockError::Invalid);
        };
        let pid = pid_field
            .strip_prefix("pid=")
            .ok_or(RuntimeLockError::Invalid)?
            .parse::<u32>()
            .map_err(|_| RuntimeLockError::Invalid)?;
        let boot = boot_field
            .strip_prefix("boot=")
            .ok_or(RuntimeLockError::Invalid)?;
        let start_ticks = start_field
            .strip_prefix("start_ticks=")
            .ok_or(RuntimeLockError::Invalid)?
            .parse::<u64>()
            .map_err(|_| RuntimeLockError::Invalid)?;
        let role = role_field
            .strip_prefix("role=")
            .ok_or(RuntimeLockError::Invalid)?;
        if boot.is_empty() || !matches!(role, "ucloned" | "slotctl") {
            return Err(RuntimeLockError::Invalid);
        }
        Ok(Self {
            pid,
            boot: boot.to_owned(),
            start_ticks,
            role: role.to_owned(),
        })
    }

    pub(super) fn body(&self) -> String {
        format!(
            "pid={}\nboot={}\nstart_ticks={}\nrole={}\n",
            self.pid, self.boot, self.start_ticks, self.role
        )
    }

    #[allow(
        clippy::unnecessary_wraps,
        reason = "Android/Linux liveness probes are fallible; macOS host proof is boot-scoped"
    )]
    pub(super) fn is_live(&self) -> Result<bool, RuntimeLockError> {
        #[cfg(any(target_os = "android", target_os = "linux"))]
        {
            if self.boot != boot_id()? {
                return Ok(false);
            }
            if process_start_ticks(self.pid)? != Some(self.start_ticks) {
                return Ok(false);
            }
            process_role_matches(self.pid, &self.role)
        }
        #[cfg(target_os = "macos")]
        {
            Ok(self.boot == boot_id())
        }
    }
}
