use std::fs;
use std::path::{Path, PathBuf};

use super::lock;
use super::storage;
use super::{EmergencyManifestError, EmergencyManifestV1};
use crate::domain::BootId;

/// Fixed filename of the atomically committed emergency manifest.
pub const MANIFEST_FILE_NAME: &str = "manifest.json";
const COMMIT_LOCK_FILE_NAME: &str = ".manifest.lock";

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
