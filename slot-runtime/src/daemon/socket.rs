use std::fs;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::UnixStream;
use std::path::Path;

use super::DaemonError;
use crate::layout::RuntimeLayout;

pub(super) fn prepare_socket_path(path: &Path, production: bool) -> Result<(), DaemonError> {
    let parent = path
        .parent()
        .filter(|candidate| !candidate.as_os_str().is_empty())
        .ok_or_else(|| DaemonError::InvalidSocketPath(path.to_path_buf()))?;
    if production {
        prepare_production_dirs(parent)?;
    }
    match fs::symlink_metadata(parent) {
        Ok(metadata) => validate_parent(parent, &metadata)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(parent)?;
            let metadata = fs::symlink_metadata(parent)?;
            validate_parent(parent, &metadata)?;
        }
        Err(error) => return Err(error.into()),
    }
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(DaemonError::SymlinkPath(path.to_path_buf()))
        }
        Ok(metadata) if !metadata.file_type().is_socket() => {
            Err(DaemonError::NonSocketPath(path.to_path_buf()))
        }
        Ok(_) => match UnixStream::connect(path) {
            Ok(_) => Err(DaemonError::SocketInUse(path.to_path_buf())),
            Err(error) if is_stale_socket_error(&error) => {
                fs::remove_file(path)?;
                Ok(())
            }
            Err(error) => Err(error.into()),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn prepare_production_dirs(run: &Path) -> Result<(), DaemonError> {
    let root = RuntimeLayout::root();
    ensure_directory(root)?;
    validate_root_directory(root)?;
    ensure_directory(run)?;
    let run_metadata = fs::symlink_metadata(run)?;
    validate_root_directory(run)?;
    if run_metadata.mode() & 0o777 != 0o700 {
        fs::set_permissions(run, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), DaemonError> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_root_directory(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::create_dir_all(path)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn validate_root_directory(path: &Path) -> Result<(), DaemonError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(DaemonError::SymlinkPath(path.to_path_buf()));
    }
    if !metadata.is_dir() {
        return Err(DaemonError::ParentNotDirectory(path.to_path_buf()));
    }
    if metadata.uid() != 0 || metadata.gid() != 0 {
        return Err(DaemonError::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "production socket directories must be owned by root:root",
        )));
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(DaemonError::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "production socket directories must not be group/other writable",
        )));
    }
    Ok(())
}

fn validate_parent(path: &Path, metadata: &fs::Metadata) -> Result<(), DaemonError> {
    if metadata.file_type().is_symlink() {
        Err(DaemonError::SymlinkPath(path.to_path_buf()))
    } else if !metadata.is_dir() {
        Err(DaemonError::ParentNotDirectory(path.to_path_buf()))
    } else {
        Ok(())
    }
}

fn is_stale_socket_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotFound
            | io::ErrorKind::BrokenPipe
    )
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn production_validator_rejects_group_writable_directory() -> Result<(), Box<dyn Error>> {
        let directory = tempfile::tempdir()?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o770))?;
        assert!(matches!(
            validate_root_directory(directory.path()),
            Err(DaemonError::Io(error)) if error.kind() == io::ErrorKind::PermissionDenied
        ));
        Ok(())
    }
}
