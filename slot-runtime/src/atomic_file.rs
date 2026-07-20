use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[allow(
    clippy::redundant_pub_crate,
    reason = "shared by sibling persistence modules while this support module stays internal"
)]
pub(crate) fn ensure_directory(path: &Path) -> io::Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "shared by sibling persistence modules while this support module stays internal"
)]
pub(crate) fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "shared by sibling persistence modules while this support module stays internal"
)]
pub(crate) fn write_new_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    ensure_directory(parent)?;
    let temporary = temporary_path(path)?;
    let result = publish_new_file(&temporary, path, parent, bytes);
    if result.is_err() {
        let _cleanup_result = fs::remove_file(&temporary);
    }
    result
}

fn publish_new_file(
    temporary: &Path,
    final_path: &Path,
    parent: &Path,
    bytes: &[u8],
) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::hard_link(temporary, final_path)?;
    sync_directory(parent)?;
    fs::remove_file(temporary)?;
    sync_directory(parent)
}

fn temporary_path(path: &Path) -> io::Result<PathBuf> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid final file name"))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(path.with_file_name(format!(
        ".{file_name}.tmp-{}-{sequence}",
        std::process::id()
    )))
}
