use std::fs::{self, File, Permissions};
use std::io;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::{Component, Path};

use crate::materializer::{ArtifactState, BackendFailure};

pub(super) fn ensure_storage_parent(
    anchor: &Path,
    parent: &Path,
    create: bool,
) -> Result<(), BackendFailure> {
    ensure_real_directory(anchor, "storage_anchor_invalid")?;
    let suffix = parent
        .strip_prefix(anchor)
        .map_err(|_| BackendFailure::new("storage_parent_outside_anchor"))?;
    let mut current = anchor.to_path_buf();
    for component in suffix.components() {
        let Component::Normal(segment) = component else {
            return Err(BackendFailure::new("storage_parent_invalid"));
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => validate_directory(&metadata, "storage_parent_invalid")?,
            Err(error) if error.kind() == io::ErrorKind::NotFound && create => {
                fs::create_dir(&current)
                    .map_err(|_| BackendFailure::new("storage_parent_create_failed"))?;
                fs::set_permissions(&current, Permissions::from_mode(0o700))
                    .map_err(|_| BackendFailure::new("storage_parent_mode_failed"))?;
                let Some(owner) = current.parent() else {
                    return Err(BackendFailure::new("storage_parent_invalid"));
                };
                sync_directory(owner)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(BackendFailure::new("storage_parent_inspect_failed")),
        }
    }
    Ok(())
}

#[allow(clippy::similar_names)]
pub(super) fn artifact_state_at(
    staging_ce: &Path,
    staging_de: &Path,
    ready_ce: &Path,
    ready_de: &Path,
) -> Result<ArtifactState, BackendFailure> {
    let staging_ce_present = artifact_present(staging_ce)?;
    let staging_de_present = artifact_present(staging_de)?;
    let ready_ce_present = artifact_present(ready_ce)?;
    let ready_de_present = artifact_present(ready_de)?;
    let staging = staging_ce_present || staging_de_present;
    let ready = ready_ce_present || ready_de_present;
    Ok(classify_artifacts(staging, ready))
}

pub(super) const fn classify_artifacts(staging: bool, ready: bool) -> ArtifactState {
    match (staging, ready) {
        (false, false) => ArtifactState::Absent,
        (true, false) => ArtifactState::StagingOnly,
        (false, true) => ArtifactState::ReadyOnly,
        (true, true) => ArtifactState::Both,
    }
}

#[allow(clippy::similar_names)]
pub(super) fn create_staging_at(
    staging_ce: &Path,
    staging_de: &Path,
    ready_ce: &Path,
    ready_de: &Path,
) -> Result<(), BackendFailure> {
    if artifact_state_at(staging_ce, staging_de, ready_ce, ready_de)? != ArtifactState::Absent {
        return Err(BackendFailure::new("artifacts_not_absent"));
    }
    create_one(staging_ce)?;
    create_one(staging_de)
}

pub(super) fn cleanup_at(paths: [&Path; 4]) -> Result<(), BackendFailure> {
    let mut first_error = None;
    for path in paths {
        let error = remove_one(path).err();
        if first_error.is_none() {
            first_error = error;
        }
    }
    first_error.map_or(Ok(()), Err)
}

#[allow(clippy::similar_names)]
pub(super) fn publish_at(
    staging_ce: &Path,
    staging_de: &Path,
    ready_ce: &Path,
    ready_de: &Path,
) -> Result<(), BackendFailure> {
    let staging_ce_present = artifact_present(staging_ce)?;
    let staging_de_present = artifact_present(staging_de)?;
    let ready_ce_present = artifact_present(ready_ce)?;
    let ready_de_present = artifact_present(ready_de)?;
    if !staging_ce_present || !staging_de_present {
        return Err(BackendFailure::new("staging_pair_incomplete"));
    }
    if ready_ce_present || ready_de_present {
        return Err(BackendFailure::new("ready_artifact_exists"));
    }
    rename_one(staging_ce, ready_ce, "publish_ce_failed")?;
    rename_one(staging_de, ready_de, "publish_de_failed")?;
    let ready_ce_present = artifact_present(ready_ce)?;
    let ready_de_present = artifact_present(ready_de)?;
    if !ready_ce_present || !ready_de_present {
        return Err(BackendFailure::new("ready_pair_incomplete"));
    }
    Ok(())
}

pub(super) fn ensure_real_directory(path: &Path, code: &str) -> Result<(), BackendFailure> {
    let metadata = fs::symlink_metadata(path).map_err(|_| BackendFailure::new(code))?;
    validate_directory(&metadata, code)
}

fn validate_directory(metadata: &fs::Metadata, code: &str) -> Result<(), BackendFailure> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        Err(BackendFailure::new(code))
    } else {
        Ok(())
    }
}

fn artifact_present(path: &Path) -> Result<bool, BackendFailure> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_directory(&metadata, "artifact_root_invalid")?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(BackendFailure::new("artifact_inspect_failed")),
    }
}

fn create_one(path: &Path) -> Result<(), BackendFailure> {
    let Some(parent) = path.parent() else {
        return Err(BackendFailure::new("staging_parent_invalid"));
    };
    ensure_real_directory(parent, "staging_parent_invalid")?;
    fs::create_dir(path).map_err(|_| BackendFailure::new("staging_create_failed"))?;
    fs::set_permissions(path, Permissions::from_mode(0o700))
        .map_err(|_| BackendFailure::new("staging_mode_failed"))?;
    sync_directory(parent)
}

fn remove_one(path: &Path) -> Result<(), BackendFailure> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(BackendFailure::new("artifact_inspect_failed")),
    };
    validate_directory(&metadata, "artifact_root_invalid")?;
    fs::remove_dir_all(path).map_err(|_| BackendFailure::new("artifact_remove_failed"))?;
    let Some(parent) = path.parent() else {
        return Err(BackendFailure::new("artifact_parent_invalid"));
    };
    sync_directory(parent)
}

fn rename_one(source: &Path, target: &Path, code: &str) -> Result<(), BackendFailure> {
    if artifact_present(target)? {
        return Err(BackendFailure::new("ready_artifact_exists"));
    }
    fs::rename(source, target).map_err(|_| BackendFailure::new(code))?;
    let Some(parent) = target.parent() else {
        return Err(BackendFailure::new("artifact_parent_invalid"));
    };
    sync_directory(parent)
}

fn sync_directory(path: &Path) -> Result<(), BackendFailure> {
    let before =
        fs::symlink_metadata(path).map_err(|_| BackendFailure::new("directory_sync_failed"))?;
    validate_directory(&before, "directory_sync_failed")?;
    let directory = File::open(path).map_err(|_| BackendFailure::new("directory_sync_failed"))?;
    let opened = directory
        .metadata()
        .map_err(|_| BackendFailure::new("directory_sync_failed"))?;
    if before.dev() != opened.dev() || before.ino() != opened.ino() {
        return Err(BackendFailure::new("directory_changed_during_sync"));
    }
    directory
        .sync_all()
        .map_err(|_| BackendFailure::new("directory_sync_failed"))
}
