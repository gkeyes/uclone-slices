use std::fs::{self, File, Metadata, OpenOptions};
#[cfg(any(target_os = "android", target_os = "linux"))]
use std::os::fd::AsRawFd as _;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

use super::{O_NOFOLLOW, TreeError};

pub(super) fn open_nofollow(path: &Path) -> Result<File, TreeError> {
    OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW)
        .open(path)
        .map_err(|_| TreeError::Io)
}

#[cfg(any(target_os = "android", target_os = "linux"))]
fn directory_fd_path(file: &File, _fallback: &Path) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn directory_fd_path(_file: &File, fallback: &Path) -> PathBuf {
    // Darwin exposes directory descriptors under `/dev/fd`, but `read_dir`
    // rejects those entries with `ENOTDIR`. Each child is still opened with
    // `O_NOFOLLOW`, while the caller compares this path with the held directory
    // descriptor before and after traversal. Android keeps the stronger
    // `/proc/self/fd` walk used in production.
    fallback.to_path_buf()
}

pub(super) fn validate_open(path: &Path, metadata: &Metadata) -> Result<(), TreeError> {
    let file = open_nofollow(path)?;
    ensure_same(metadata, &file.metadata().map_err(|_| TreeError::Io)?)
}

pub(super) fn directory_path(file: &File, fallback: &Path) -> PathBuf {
    directory_fd_path(file, fallback)
}

pub(super) fn ensure_path_and_file(
    path: &Path,
    expected: &Metadata,
    file: &File,
) -> Result<(), TreeError> {
    ensure_same(expected, &file.metadata().map_err(|_| TreeError::Io)?)?;
    ensure_path_metadata(path, expected)
}

pub(super) fn ensure_path_metadata(path: &Path, expected: &Metadata) -> Result<(), TreeError> {
    ensure_same(
        expected,
        &fs::symlink_metadata(path).map_err(|_| TreeError::Io)?,
    )
}

pub(super) fn ensure_same(expected: &Metadata, observed: &Metadata) -> Result<(), TreeError> {
    let same = expected.dev() == observed.dev()
        && expected.ino() == observed.ino()
        && expected.mode() == observed.mode()
        && expected.nlink() == observed.nlink()
        && expected.uid() == observed.uid()
        && expected.gid() == observed.gid()
        && expected.rdev() == observed.rdev()
        && expected.size() == observed.size()
        && expected.mtime() == observed.mtime()
        && expected.mtime_nsec() == observed.mtime_nsec()
        && expected.ctime() == observed.ctime()
        && expected.ctime_nsec() == observed.ctime_nsec();
    if same {
        Ok(())
    } else {
        Err(TreeError::MetadataChanged)
    }
}
