use std::fs::{self, OpenOptions};
use std::io::Read as _;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::Path;

use super::super::{EmergencyManifestError, no_follow_flag};
use super::{MAX_RECORD_BYTES, MAX_RECORD_BYTES_U64, validate_directory};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "the sibling manifest Store consumes this private reader result"
)]
pub(crate) struct RecordFence {
    pub(crate) root_device: u64,
    pub(crate) root_inode: u64,
    pub(crate) manifest_device: u64,
    pub(crate) manifest_inode: u64,
    pub(crate) manifest_length: u64,
}

#[derive(Debug, PartialEq, Eq)]
#[allow(
    clippy::redundant_pub_crate,
    reason = "the sibling manifest Store consumes this private reader result"
)]
pub(crate) struct ReadRecord {
    pub(crate) bytes: Vec<u8>,
    pub(crate) fence: RecordFence,
}

pub(super) fn open_existing_root(path: &Path) -> Result<u32, EmergencyManifestError> {
    if !path.is_absolute() {
        return Err(EmergencyManifestError::Corrupt(format!(
            "untrusted emergency manifest root {}",
            path.display()
        )));
    }
    let root_metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(EmergencyManifestError::MissingRoot {
                path: path.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(EmergencyManifestError::io(
                "inspect emergency manifest root",
                path,
                source,
            ));
        }
    };
    let parent = path.parent().ok_or_else(|| {
        EmergencyManifestError::Corrupt("emergency manifest root has no parent".to_owned())
    })?;
    let parent_metadata = fs::symlink_metadata(parent)
        .map_err(|source| EmergencyManifestError::io("inspect manifest parent", parent, source))?;
    let owner_uid = parent_metadata.uid();
    #[cfg(target_os = "android")]
    if owner_uid != 0 {
        return Err(EmergencyManifestError::Corrupt(
            "emergency manifest root is not root-owned".to_owned(),
        ));
    }
    validate_directory(parent, owner_uid)?;
    validate_directory(path, owner_uid)?;
    let opened = fs::symlink_metadata(path).map_err(|source| {
        EmergencyManifestError::io("recheck emergency manifest root", path, source)
    })?;
    if root_metadata.dev() != opened.dev() || root_metadata.ino() != opened.ino() {
        return Err(EmergencyManifestError::Corrupt(
            "emergency manifest root changed during inspection".to_owned(),
        ));
    }
    Ok(owner_uid)
}

pub(super) fn read_record_with_fence(
    root: &Path,
    path: &Path,
    owner_uid: u32,
) -> Result<ReadRecord, EmergencyManifestError> {
    let root_before = directory_metadata(root, owner_uid)?;
    let before = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(EmergencyManifestError::MissingManifest {
                path: path.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(EmergencyManifestError::io(
                "inspect manifest record",
                path,
                source,
            ));
        }
    };
    super::validate_record_metadata(&before, path)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow_flag())
        .open(path)
        .map_err(|source| EmergencyManifestError::io("open manifest record", path, source))?;
    let opened = file.metadata().map_err(|source| {
        EmergencyManifestError::io("inspect opened manifest record", path, source)
    })?;
    super::validate_record_metadata(&opened, path)?;
    if !same_file(&before, &opened) || opened.uid() != owner_uid {
        return Err(super::untrusted_record(path));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(opened.len()).unwrap_or(MAX_RECORD_BYTES));
    std::io::Read::by_ref(&mut file)
        .take(MAX_RECORD_BYTES_U64.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| EmergencyManifestError::io("read manifest record", path, source))?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(EmergencyManifestError::BoundExceeded("serialized record"));
    }
    let after = fs::symlink_metadata(path)
        .map_err(|source| EmergencyManifestError::io("recheck manifest record", path, source))?;
    super::validate_record_metadata(&after, path)?;
    if !same_file(&opened, &after) || opened.len() != after.len() || after.uid() != owner_uid {
        return Err(EmergencyManifestError::Corrupt(
            "manifest changed during read".to_owned(),
        ));
    }
    let root_after = directory_metadata(root, owner_uid)?;
    if !same_file(&root_before, &root_after) {
        return Err(EmergencyManifestError::Corrupt(
            "manifest root changed during read".to_owned(),
        ));
    }
    Ok(ReadRecord {
        bytes,
        fence: RecordFence {
            root_device: root_after.dev(),
            root_inode: root_after.ino(),
            manifest_device: after.dev(),
            manifest_inode: after.ino(),
            manifest_length: after.len(),
        },
    })
}

fn directory_metadata(path: &Path, owner_uid: u32) -> Result<fs::Metadata, EmergencyManifestError> {
    validate_directory(path, owner_uid)?;
    fs::symlink_metadata(path)
        .map_err(|source| EmergencyManifestError::io("inspect manifest directory", path, source))
}

fn same_file(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        reason = "fixture setup must abort this unit test"
    )]

    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    use tempfile::TempDir;

    use crate::domain::{BootId, PackageName};
    use crate::emergency_manifest::{
        ContainmentObligation, EmergencyManifestError, EmergencyManifestStore, EmergencyManifestV1,
    };

    #[test]
    fn open_existing_missing_root_never_creates_root_or_lock() {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("emergency-manifest");
        let error = EmergencyManifestStore::open_existing(&root).unwrap_err();

        assert!(matches!(error, EmergencyManifestError::MissingRoot { .. }));
        assert!(!root.exists());
        assert!(!root.join(".manifest.lock").exists());
    }

    #[test]
    fn existing_root_without_record_is_distinct_and_read_only() {
        let parent = TempDir::new().unwrap();
        std::fs::set_permissions(parent.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let root = parent.path().join("emergency-manifest");
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let store = EmergencyManifestStore::open_existing(&root).unwrap();
        let error = store.load_strict().unwrap_err();

        assert!(matches!(
            error,
            EmergencyManifestError::MissingManifest { .. }
        ));
        assert!(!root.join(".manifest.lock").exists());
    }

    #[test]
    fn strict_load_returns_raw_digest_and_stable_fence() {
        let root = TempDir::new().unwrap();
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let store = EmergencyManifestStore::new(root.path()).unwrap();
        let manifest = EmergencyManifestV1::new(
            BootId::parse("boot-00000001").unwrap(),
            1,
            vec![crate::emergency_manifest::PackageContainment::new(
                PackageName::parse("com.example.app").unwrap(),
                ContainmentObligation::Held,
            )],
        )
        .unwrap();
        store.commit(&manifest).unwrap();

        let bytes = std::fs::read(store.manifest_path()).unwrap();
        let record = store.load_strict().unwrap();
        let manifest_metadata = std::fs::symlink_metadata(store.manifest_path()).unwrap();
        let root_metadata = std::fs::symlink_metadata(store.root()).unwrap();

        assert_eq!(record.manifest(), &manifest);
        assert_eq!(record.raw_sha256(), crate::integrity::digest_bytes(&bytes));
        assert_eq!(record.fence().root_device(), root_metadata.dev());
        assert_eq!(record.fence().root_inode(), root_metadata.ino());
        assert_eq!(record.fence().manifest_device(), manifest_metadata.dev());
        assert_eq!(record.fence().manifest_inode(), manifest_metadata.ino());
        let byte_length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        assert_eq!(record.fence().manifest_length(), byte_length);
    }
}
