use std::fs;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use crate::domain::{ManagedPackage, PackageName};
use crate::store_security;

use super::scan::{
    scan_package_names, validate_package_directory, validate_package_directory_for_create,
    validate_store_layout,
};
use super::wire::EnrollmentRecord;
use super::{EnrollmentError, SCHEMA_VERSION, map_security, validate_base};

#[doc = "Filesystem-backed immutable package enrollment store."]
#[derive(Debug, Clone)]
pub struct EnrollmentStore {
    root: PathBuf,
    packages: PathBuf,
    owner_uid: u32,
}

impl EnrollmentStore {
    #[doc = "Creates or opens the root-only enrollment directory."]
    pub fn new(root: impl AsRef<Path>) -> Result<Self, EnrollmentError> {
        let root = root.as_ref().to_path_buf();
        let owner_uid = store_security::initialize_root(&root, "enrollment")
            .map_err(|error| map_security("initialize enrollment root", &root, error))?;
        let packages = root.join("packages");
        store_security::ensure_child_directory(&packages, owner_uid, "enrollment")
            .map_err(|error| map_security("create enrollment packages", &packages, error))?;
        validate_store_layout(&root, &packages, owner_uid)?;
        Ok(Self {
            root,
            packages,
            owner_uid,
        })
    }

    #[doc = "Publishes one base-only enrollment without replacing an existing record."]
    pub fn create(&self, managed: &ManagedPackage) -> Result<(), EnrollmentError> {
        self.validate_store()?;
        validate_base(managed).map_err(EnrollmentError::Invalid)?;
        let package_dir = self.package_path(managed.package_name());
        store_security::ensure_child_directory(&package_dir, self.owner_uid, "enrollment package")
            .map_err(|error| map_security("create package enrollment", &package_dir, error))?;
        if validate_package_directory_for_create(&package_dir, self.owner_uid)?.is_some() {
            return Err(EnrollmentError::AlreadyExists(
                managed.package_name().clone(),
            ));
        }
        let path = package_dir.join("enrollment.json");
        let mut record = EnrollmentRecord {
            schema_version: SCHEMA_VERSION,
            managed: managed.clone(),
            sha256: String::new(),
        };
        record.sha256 = record.digest()?;
        let bytes = serde_json::to_vec(&record)?;
        store_security::write_new_record(&path, &bytes, self.owner_uid, "enrollment record")
            .map_err(|error| map_security("publish enrollment", &path, error))
    }

    #[doc = "Loads and verifies a package enrollment when present."]
    pub fn load(
        &self,
        package_name: &PackageName,
    ) -> Result<Option<ManagedPackage>, EnrollmentError> {
        self.validate_store()?;
        let package_dir = self.package_path(package_name);
        match fs::symlink_metadata(&package_dir) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(EnrollmentError::io(
                    "inspect package enrollment",
                    &package_dir,
                    source,
                ));
            }
            Ok(_) => {}
        }
        validate_package_directory(&package_dir, self.owner_uid)?;
        let path = package_dir.join("enrollment.json");
        let bytes = store_security::read_record(&path, self.owner_uid, "enrollment record")
            .map_err(|error| map_security("read enrollment", &path, error))?;
        let record = serde_json::from_slice::<EnrollmentRecord>(&bytes)
            .map_err(|source| EnrollmentError::Corrupt(source.to_string()))?;
        record.verify()?;
        if record.managed.package_name() != package_name {
            return Err(EnrollmentError::Corrupt(
                "enrollment package does not match its directory".to_owned(),
            ));
        }
        Ok(Some(record.managed))
    }

    #[doc = "Scans recognizable package names without opening enrollment records."]
    pub fn package_names(&self) -> Result<super::EnrollmentNameScan, EnrollmentError> {
        self.validate_store()?;
        scan_package_names(&self.packages, self.owner_uid)
    }

    #[doc = "Enumerates every enrollment while rejecting unknown package artifacts."]
    pub fn list(&self) -> Result<Vec<ManagedPackage>, EnrollmentError> {
        let scan = self.package_names()?;
        if scan.corrupt_artifact() {
            return Err(EnrollmentError::Corrupt(
                "unexpected enrollment artifact".to_owned(),
            ));
        }
        scan.package_names()
            .iter()
            .map(|package_name| {
                validate_package_directory(&self.package_path(package_name), self.owner_uid)?;
                self.load(package_name)?.ok_or_else(|| {
                    EnrollmentError::Corrupt(format!(
                        "missing enrollment record for {package_name}"
                    ))
                })
            })
            .collect()
    }

    #[doc = "Returns the enrollment root directory."]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub(super) fn open_existing(root: &Path) -> Result<Self, EnrollmentError> {
        let parent = root.parent().ok_or_else(|| {
            EnrollmentError::Corrupt("enrollment root has no parent directory".to_owned())
        })?;
        let parent_metadata = fs::symlink_metadata(parent)
            .map_err(|source| EnrollmentError::io("inspect enrollment parent", parent, source))?;
        let owner_uid = parent_metadata.uid();
        store_security::validate_directory(parent, owner_uid, "enrollment")
            .map_err(|error| map_security("validate enrollment parent", parent, error))?;
        store_security::validate_directory(root, owner_uid, "enrollment")
            .map_err(|error| map_security("validate enrollment root", root, error))?;
        let root = root.to_path_buf();
        let packages = root.join("packages");
        store_security::validate_directory(&packages, owner_uid, "enrollment")
            .map_err(|error| map_security("validate enrollment packages", &packages, error))?;
        validate_store_layout(&root, &packages, owner_uid)?;
        Ok(Self {
            root,
            packages,
            owner_uid,
        })
    }

    pub(super) fn package_path(&self, package_name: &PackageName) -> PathBuf {
        self.packages.join(package_name.as_str())
    }

    pub(super) const fn owner_uid(&self) -> u32 {
        self.owner_uid
    }

    fn validate_store(&self) -> Result<(), EnrollmentError> {
        validate_store_layout(&self.root, &self.packages, self.owner_uid)
    }
}
