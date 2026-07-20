use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::shape;
use super::{
    FILE_MODE, MAX_RECORD_BYTES, MAX_RECORD_BYTES_U64, StoreSecurityError, no_follow_flag,
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[doc = "Publishes beneath a prevalidated owner-only `0700` parent without creating or repairing it. The safe standard-library implementation relies on that parent excluding unprivileged path replacement; it does not claim resistance to a second root writer without `openat2`."]
pub(crate) fn write_new_record(
    path: &Path,
    bytes: &[u8],
    owner_uid: u32,
    kind: &str,
) -> Result<(), StoreSecurityError> {
    validate_record_bytes(bytes, kind)?;
    reject_existing(path, kind)?;
    let parent = path
        .parent()
        .ok_or_else(|| StoreSecurityError::Corrupt(format!("invalid {kind} path")))?;
    shape::validate_directory(parent, owner_uid, kind)?;
    let temporary = temporary_path(path, kind)?;
    let result = publish(&temporary, path, parent, bytes, owner_uid, kind);
    if result.is_err() {
        let _cleanup_result = fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn read_record(
    path: &Path,
    owner_uid: u32,
    kind: &str,
) -> Result<Vec<u8>, StoreSecurityError> {
    let before = fs::symlink_metadata(path).map_err(StoreSecurityError::Io)?;
    validate_record_metadata(&before, owner_uid, path, kind)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(StoreSecurityError::Io)?;
    let opened = file.metadata().map_err(StoreSecurityError::Io)?;
    validate_record_metadata(&opened, owner_uid, path, kind)?;
    if !same_file(&before, &opened) {
        return Err(changed(path, kind));
    }
    let capacity = usize::try_from(opened.len()).map_err(|_| untrusted_record(path, kind))?;
    let mut bytes = Vec::with_capacity(capacity);
    std::io::Read::by_ref(&mut file)
        .take(MAX_RECORD_BYTES_U64.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(StoreSecurityError::Io)?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(untrusted_record(path, kind));
    }
    let after = fs::symlink_metadata(path).map_err(StoreSecurityError::Io)?;
    validate_record_metadata(&after, owner_uid, path, kind)?;
    if !same_file(&opened, &after) || opened.len() != after.len() {
        return Err(changed(path, kind));
    }
    Ok(bytes)
}

pub(crate) fn validate_record(
    path: &Path,
    owner_uid: u32,
    kind: &str,
) -> Result<(), StoreSecurityError> {
    let metadata = fs::symlink_metadata(path).map_err(StoreSecurityError::Io)?;
    validate_record_metadata(&metadata, owner_uid, path, kind)
}

pub(crate) fn validate_record_bytes(bytes: &[u8], kind: &str) -> Result<(), StoreSecurityError> {
    if bytes.len() <= MAX_RECORD_BYTES {
        Ok(())
    } else {
        Err(StoreSecurityError::Corrupt(format!(
            "untrusted {kind}: serialized record exceeds the bounded size"
        )))
    }
}

fn publish(
    temporary: &Path,
    final_path: &Path,
    parent: &Path,
    bytes: &[u8],
    owner_uid: u32,
    kind: &str,
) -> Result<(), StoreSecurityError> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(FILE_MODE)
        .custom_flags(no_follow_flag())
        .open(temporary)
        .map_err(StoreSecurityError::Io)?;
    file.set_permissions(fs::Permissions::from_mode(FILE_MODE))
        .map_err(StoreSecurityError::Io)?;
    validate_open_record(&file, owner_uid, temporary, kind)?;
    file.write_all(bytes).map_err(StoreSecurityError::Io)?;
    file.sync_all().map_err(StoreSecurityError::Io)?;
    drop(file);
    fs::hard_link(temporary, final_path).map_err(StoreSecurityError::Io)?;
    shape::sync_directory(parent, owner_uid, kind)?;
    fs::remove_file(temporary).map_err(StoreSecurityError::Io)?;
    shape::sync_directory(parent, owner_uid, kind)?;
    validate_record(final_path, owner_uid, kind)
}

fn validate_open_record(
    file: &File,
    owner_uid: u32,
    path: &Path,
    kind: &str,
) -> Result<(), StoreSecurityError> {
    let metadata = file.metadata().map_err(StoreSecurityError::Io)?;
    validate_record_metadata(&metadata, owner_uid, path, kind)
}

fn validate_record_metadata(
    metadata: &fs::Metadata,
    owner_uid: u32,
    path: &Path,
    kind: &str,
) -> Result<(), StoreSecurityError> {
    if metadata.file_type().is_file()
        && metadata.uid() == owner_uid
        && metadata.mode() & 0o7777 == FILE_MODE
        && metadata.nlink() == 1
        && metadata.len() <= MAX_RECORD_BYTES_U64
    {
        Ok(())
    } else {
        Err(untrusted_record(path, kind))
    }
}

fn reject_existing(path: &Path, kind: &str) -> Result<(), StoreSecurityError> {
    match fs::symlink_metadata(path) {
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(StoreSecurityError::Corrupt(format!(
            "{kind} generation already exists"
        ))),
        Err(source) => Err(StoreSecurityError::Io(source)),
    }
}

fn temporary_path(path: &Path, kind: &str) -> Result<PathBuf, StoreSecurityError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| StoreSecurityError::Corrupt(format!("invalid {kind} name")))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(path.with_file_name(format!(".{name}.tmp-{}-{sequence}", std::process::id())))
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn untrusted_record(path: &Path, kind: &str) -> StoreSecurityError {
    StoreSecurityError::Corrupt(format!("untrusted {kind} {}", path.display()))
}

fn changed(path: &Path, kind: &str) -> StoreSecurityError {
    StoreSecurityError::Corrupt(format!(
        "{kind} changed during validation: {}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "fixture setup must abort the individual security helper test"
    )]

    use super::*;

    use tempfile::TempDir;

    fn secure_temp_dir(root: &TempDir) {
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[test]
    fn rejects_a_record_owned_by_a_different_expected_uid() {
        let root = TempDir::new().unwrap();
        secure_temp_dir(&root);
        let path = root.path().join("record.json");
        let owner_uid = fs::symlink_metadata(root.path()).unwrap().uid();
        write_new_record(&path, b"{}", owner_uid, "test record").unwrap();

        let error = read_record(&path, owner_uid.wrapping_add(1), "test record").unwrap_err();

        assert!(matches!(error, StoreSecurityError::Corrupt(_)));
    }

    #[test]
    fn publisher_does_not_create_a_missing_parent() {
        let root = TempDir::new().unwrap();
        secure_temp_dir(&root);
        let missing = root.path().join("missing");
        let owner_uid = fs::symlink_metadata(root.path()).unwrap().uid();

        let error = write_new_record(
            &missing.join("record.json"),
            b"{}",
            owner_uid,
            "test record",
        )
        .unwrap_err();

        assert!(matches!(error, StoreSecurityError::Io(_)));
        assert!(!missing.exists());
    }

    #[test]
    fn publisher_does_not_repair_an_unsafe_parent() {
        let root = TempDir::new().unwrap();
        secure_temp_dir(&root);
        let parent = root.path().join("unsafe");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
        let owner_uid = fs::symlink_metadata(root.path()).unwrap().uid();

        let error = write_new_record(&parent.join("record.json"), b"{}", owner_uid, "test record")
            .unwrap_err();

        assert!(matches!(error, StoreSecurityError::Corrupt(_)));
        assert_eq!(
            fs::symlink_metadata(&parent).unwrap().mode() & 0o7777,
            0o755
        );
        assert!(!parent.join("record.json").exists());
    }
}
