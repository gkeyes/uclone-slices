#[cfg(any(target_os = "android", target_os = "linux"))]
use std::fs::{self, File};
#[cfg(any(target_os = "android", target_os = "linux"))]
use std::io::{self, Read};
#[cfg(any(target_os = "android", target_os = "linux"))]
use std::path::PathBuf;

#[cfg(any(target_os = "android", target_os = "linux"))]
use super::RuntimeLockError;

#[cfg(any(target_os = "android", target_os = "linux"))]
pub(super) fn process_role_matches(pid: u32, role: &str) -> Result<bool, RuntimeLockError> {
    let command_path = PathBuf::from(format!("/proc/{pid}/cmdline"));
    let mut command = Vec::new();
    match File::open(command_path) {
        Ok(file) => {
            file.take(256).read_to_end(&mut command)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(RuntimeLockError::Io(error)),
    }
    let suffix = format!("/{role}");
    Ok(command
        .split(|byte| *byte == 0)
        .any(|part| part == role.as_bytes() || part.ends_with(suffix.as_bytes())))
}

#[cfg(any(target_os = "android", target_os = "linux"))]
pub(super) fn process_start_ticks(pid: u32) -> Result<Option<u64>, RuntimeLockError> {
    let path = PathBuf::from(format!("/proc/{pid}/stat"));
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RuntimeLockError::Io(error)),
    };
    let text = core::str::from_utf8(&bytes).map_err(|_| RuntimeLockError::Invalid)?;
    let command_end = text.rfind(") ").ok_or(RuntimeLockError::Invalid)?;
    let mut fields = text
        .get(command_end + 2..)
        .ok_or(RuntimeLockError::Invalid)?
        .split_whitespace();
    fields.next().ok_or(RuntimeLockError::Invalid)?;
    let start_ticks = fields
        .nth(18)
        .ok_or(RuntimeLockError::Invalid)?
        .parse::<u64>()
        .map_err(|_| RuntimeLockError::Invalid)?;
    Ok(Some(start_ticks))
}

#[cfg(any(target_os = "android", target_os = "linux"))]
pub(super) fn boot_id() -> Result<String, RuntimeLockError> {
    let mut bytes = Vec::new();
    File::open("/proc/sys/kernel/random/boot_id")?
        .take(65)
        .read_to_end(&mut bytes)?;
    let text = core::str::from_utf8(&bytes).map_err(|_| RuntimeLockError::Invalid)?;
    let value = text.trim();
    if value.is_empty()
        || value.len() > 64
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || byte == b'-'))
    {
        return Err(RuntimeLockError::Invalid);
    }
    Ok(value.to_owned())
}

#[cfg(target_os = "macos")]
pub(super) fn boot_id() -> String {
    "host-test-boot".to_owned()
}
