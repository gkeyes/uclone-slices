use std::fs;
use std::path::{Path, PathBuf};

use super::lock;
use super::storage;
use super::{EmergencyManifestError, EmergencyManifestV1};
use crate::domain::BootId;
use crate::integrity::digest_bytes;

/// Fixed filename of the atomically committed emergency manifest.
pub const MANIFEST_FILE_NAME: &str = "manifest.json";
const COMMIT_LOCK_FILE_NAME: &str = ".manifest.lock";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc = "Stable filesystem fence captured with a strict manifest read."]
pub struct EmergencyManifestFence {
    root_device: u64,
    root_inode: u64,
    manifest_device: u64,
    manifest_inode: u64,
    manifest_length: u64,
}

impl EmergencyManifestFence {
    #[doc = "Returns the device containing the manifest root."]
    pub const fn root_device(self) -> u64 {
        self.root_device
    }

    #[doc = "Returns the root directory inode."]
    pub const fn root_inode(self) -> u64 {
        self.root_inode
    }

    #[doc = "Returns the device containing the manifest record."]
    pub const fn manifest_device(self) -> u64 {
        self.manifest_device
    }

    #[doc = "Returns the manifest record inode."]
    pub const fn manifest_inode(self) -> u64 {
        self.manifest_inode
    }

    #[doc = "Returns the exact serialized record length."]
    pub const fn manifest_length(self) -> u64 {
        self.manifest_length
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[doc = "Validated manifest paired with its exact raw digest and filesystem fence."]
pub struct EmergencyManifestRecord {
    manifest: EmergencyManifestV1,
    raw_sha256: String,
    fence: EmergencyManifestFence,
}

impl EmergencyManifestRecord {
    #[doc = "Returns the validated manifest."]
    pub const fn manifest(&self) -> &EmergencyManifestV1 {
        &self.manifest
    }

    #[doc = "Returns the lower-case SHA-256 digest of the raw record bytes."]
    pub fn raw_sha256(&self) -> &str {
        &self.raw_sha256
    }

    #[doc = "Returns the filesystem fence captured during the read."]
    pub const fn fence(&self) -> EmergencyManifestFence {
        self.fence
    }
}

/// Filesystem-backed, single-record emergency manifest store.
#[derive(Debug, Clone)]
pub struct EmergencyManifestStore {
    root: PathBuf,
    path: PathBuf,
    lock_path: PathBuf,
    owner_uid: u32,
}

impl EmergencyManifestStore {
    /// Creates or opens a root-only manifest directory.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, EmergencyManifestError> {
        let root = root.as_ref().to_path_buf();
        let owner_uid = storage::initialize_root(&root)?;
        let lock_path = root.join(COMMIT_LOCK_FILE_NAME);
        lock::initialize(&lock_path, owner_uid)?;
        let store = Self {
            path: root.join(MANIFEST_FILE_NAME),
            lock_path,
            root,
            owner_uid,
        };
        store.validate_store()?;
        Ok(store)
    }

    #[doc = "Opens an existing root without creating or modifying any path."]
    pub fn open_existing(root: impl AsRef<Path>) -> Result<Self, EmergencyManifestError> {
        let root = root.as_ref().to_path_buf();
        let owner_uid = storage::open_existing_root(&root)?;
        Ok(Self {
            path: root.join(MANIFEST_FILE_NAME),
            lock_path: root.join(COMMIT_LOCK_FILE_NAME),
            root,
            owner_uid,
        })
    }

    /// Loads and verifies the last committed manifest, ignoring partial temporary files.
    pub fn load(&self) -> Result<Option<EmergencyManifestV1>, EmergencyManifestError> {
        self.validate_store()?;
        match fs::symlink_metadata(&self.path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(EmergencyManifestError::io(
                "inspect emergency manifest",
                &self.path,
                source,
            )),
            Ok(_) => {
                let bytes = storage::read_record(&self.path, self.owner_uid)?;
                let manifest =
                    serde_json::from_slice::<EmergencyManifestV1>(&bytes).map_err(|source| {
                        EmergencyManifestError::Corrupt(format!(
                            "JSON or manifest validation failed: {source}"
                        ))
                    })?;
                manifest.validate()?;
                Ok(Some(manifest))
            }
        }
    }

    #[doc = "Loads the committed manifest with a bounded no-follow read and stable fence."]
    pub fn load_strict(&self) -> Result<EmergencyManifestRecord, EmergencyManifestError> {
        self.validate_store()?;
        let record = storage::read_record_with_fence(&self.root, &self.path, self.owner_uid)?;
        let manifest =
            serde_json::from_slice::<EmergencyManifestV1>(&record.bytes).map_err(|source| {
                EmergencyManifestError::Corrupt(format!(
                    "JSON or manifest validation failed: {source}"
                ))
            })?;
        manifest.validate()?;
        Ok(EmergencyManifestRecord {
            manifest,
            raw_sha256: digest_bytes(&record.bytes),
            fence: EmergencyManifestFence {
                root_device: record.fence.root_device,
                root_inode: record.fence.root_inode,
                manifest_device: record.fence.manifest_device,
                manifest_inode: record.fence.manifest_inode,
                manifest_length: record.fence.manifest_length,
            },
        })
    }

    /// Loads a manifest only when it belongs to the caller's current boot epoch.
    pub fn load_for_boot(
        &self,
        boot_id: &BootId,
    ) -> Result<Option<EmergencyManifestV1>, EmergencyManifestError> {
        let manifest = self.load()?;
        if let Some(manifest) = &manifest
            && manifest.boot_id() != boot_id
        {
            return Err(EmergencyManifestError::StaleBoot {
                expected: boot_id.as_str().to_owned(),
                actual: manifest.boot_id().as_str().to_owned(),
            });
        }
        Ok(manifest)
    }

    /// Atomically commits the next manifest generation and verifies it by reading it back.
    pub fn commit(&self, manifest: &EmergencyManifestV1) -> Result<(), EmergencyManifestError> {
        let _commit_lock = lock::acquire(&self.lock_path, self.owner_uid)?;
        manifest.validate()?;
        self.validate_store()?;
        if let Some(previous) = self.load()? {
            if previous.boot_id() == manifest.boot_id() {
                manifest.validate_transition_from(&previous)?;
            } else {
                manifest.validate_new_boot_from(&previous)?;
            }
        } else if manifest.generation() != 1 {
            return Err(EmergencyManifestError::StaleGeneration {
                expected: 1,
                actual: manifest.generation(),
            });
        } else {
            manifest.validate_new_boot_root()?;
        }
        let bytes = serde_json::to_vec(manifest)?;
        storage::write_atomic(&self.path, &self.root, &bytes, self.owner_uid)?;
        let roundtrip = self.load()?.ok_or_else(|| {
            EmergencyManifestError::Corrupt(
                "committed emergency manifest disappeared after publication".to_owned(),
            )
        })?;
        if roundtrip != *manifest {
            return Err(EmergencyManifestError::Corrupt(
                "committed emergency manifest differs after readback".to_owned(),
            ));
        }
        Ok(())
    }

    /// Returns the root-only directory containing the committed record.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the fixed committed manifest path.
    pub fn manifest_path(&self) -> &Path {
        &self.path
    }

    fn validate_store(&self) -> Result<(), EmergencyManifestError> {
        storage::validate_directory(&self.root, self.owner_uid)
    }
}
