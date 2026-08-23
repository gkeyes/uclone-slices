use std::fmt::Display;
use std::fs::{self, File};
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

pub(super) const BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";
const BOOT_RECONCILE_MARKER: &str = "last-boot-reconcile-v1";

pub(super) fn prepare_runtime_root(root: &Path) -> Result<(), std::io::Error> {
    let metadata = match fs::symlink_metadata(root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(root)?;
            fs::symlink_metadata(root)?
        }
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Runtime root is not a real directory",
        ));
    }
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

pub(super) fn wait_for_user0_ready() {
    loop {
        let ce_available = command_stdout("/system/bin/getprop", &["sys.user.0.ce_available"])
            .is_some_and(|value| value.trim() == "true");
        let running_unlocked = command_stdout(
            "/system/bin/cmd",
            &["activity", "get-started-user-state", "0"],
        )
        .is_some_and(|value| value.trim() == "RUNNING_UNLOCKED");
        if ce_available && running_unlocked {
            return;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn command_stdout(program: &str, arguments: &[&str]) -> Option<String> {
    let output = Command::new(program).args(arguments).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(super) fn reconcile_once_per_boot<E>(
    root: &Path,
    boot_id: &str,
    reconcile: impl FnOnce() -> Result<(), E>,
) -> Result<bool, Box<dyn std::error::Error>>
where
    E: Display,
{
    if boot_id.is_empty() || boot_id.contains(['\n', '\r']) {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "boot ID is invalid",
        )));
    }
    let marker = root.join(BOOT_RECONCILE_MARKER);
    match fs::read_to_string(&marker) {
        Ok(previous) if previous.trim() == boot_id => return Ok(false),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(Box::new(error)),
    }
    reconcile().map_err(|error| {
        Box::new(std::io::Error::other(error.to_string())) as Box<dyn std::error::Error>
    })?;
    write_boot_marker(root, boot_id)?;
    Ok(true)
}

fn write_boot_marker(root: &Path, boot_id: &str) -> Result<(), std::io::Error> {
    let marker = root.join(BOOT_RECONCILE_MARKER);
    let temporary = root.join(format!(
        ".{BOOT_RECONCILE_MARKER}.tmp-{}",
        std::process::id()
    ));
    let mut file = File::create(&temporary)?;
    writeln!(file, "{boot_id}")?;
    file.sync_all()?;
    fs::rename(&temporary, marker)?;
    File::open(root)?.sync_all()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn boot_reconcile_marker_runs_once_for_each_boot_id() {
        let root = tempfile::tempdir().unwrap();
        let mut calls = 0_u8;

        assert!(
            reconcile_once_per_boot(root.path(), "boot-a", || {
                calls += 1;
                Ok::<(), std::io::Error>(())
            })
            .unwrap()
        );
        assert!(
            !reconcile_once_per_boot(root.path(), "boot-a", || {
                calls += 1;
                Ok::<(), std::io::Error>(())
            })
            .unwrap()
        );
        assert!(
            reconcile_once_per_boot(root.path(), "boot-b", || {
                calls += 1;
                Ok::<(), std::io::Error>(())
            })
            .unwrap()
        );

        assert_eq!(calls, 2);
        assert_eq!(
            fs::read_to_string(root.path().join(BOOT_RECONCILE_MARKER)).unwrap(),
            "boot-b\n"
        );
    }

    #[test]
    fn failed_boot_reconcile_is_not_marked_complete() {
        let root = tempfile::tempdir().unwrap();

        let result = reconcile_once_per_boot(root.path(), "boot-a", || {
            Err::<(), _>(std::io::Error::other("injected failure"))
        });

        assert!(result.is_err());
        assert!(!root.path().join(BOOT_RECONCILE_MARKER).exists());
    }
}
