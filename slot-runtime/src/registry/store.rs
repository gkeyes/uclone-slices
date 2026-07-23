use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::{PackageRevision, RegistryError, cache};
use crate::domain::PackageName;
use crate::store_security::{self, StoreSecurityError};

#[doc = "Filesystem-backed append-only package Registry."]
#[derive(Debug, Clone)]
pub struct RegistryStore {
    root: PathBuf,
    packages: PathBuf,
    owner_uid: u32,
    heads: Arc<Mutex<BTreeMap<PackageName, cache::CachedChain>>>,
}

impl RegistryStore {
    #[doc = "Creates or opens a root-only Registry directory."]
    pub fn new(root: impl AsRef<Path>) -> Result<Self, RegistryError> {
        let root = root.as_ref().to_path_buf();
        let owner_uid = store_security::initialize_root(&root, "Registry")
            .map_err(|error| map_security("initialize Registry root", &root, error))?;
        let packages = root.join("packages");
        secure_directory(&packages, owner_uid)?;
        Ok(Self {
            root,
            packages,
            owner_uid,
            heads: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    #[doc = "Publishes the next immutable revision for one package."]
    pub fn append(&self, draft: &PackageRevision) -> Result<PackageRevision, RegistryError> {
        self.validate_store()?;
        let package = self.package_path(draft.package_name());
        secure_directory(&package, self.owner_uid)?;
        let revisions = self.revisions_path(draft.package_name());
        secure_directory(&revisions, self.owner_uid)?;
        let mut heads = self
            .heads
            .lock()
            .map_err(|_| RegistryError::Corrupt("Registry head cache poisoned".to_owned()))?;
        let previous = if let Some(cached) = heads.get(draft.package_name()) {
            cache::validate(&revisions, self.owner_uid, cached)?;
            cached.head().cloned()
        } else {
            let loaded = self.load_all(draft.package_name())?;
            let previous = loaded.last().cloned();
            let cached = cache::snapshot(&revisions, self.owner_uid, previous.as_ref())?;
            heads.insert(draft.package_name().clone(), cached);
            previous
        };
        let revision = draft.publish_after(previous.as_ref())?;
        let path = revisions.join(revision_file_name(revision.generation));
        let bytes = serde_json::to_vec(&revision)?;
        if let Err(error) =
            store_security::write_new_record(&path, &bytes, self.owner_uid, "Registry revision")
        {
            heads.remove(draft.package_name());
            return Err(map_security("publish Registry revision", &path, error));
        }
        let Some(cached) = heads.get_mut(draft.package_name()) else {
            return Err(RegistryError::Corrupt(
                "Registry head cache disappeared".to_owned(),
            ));
        };
        if let Err(error) = cache::advance(cached, &path, self.owner_uid, revision.clone()) {
            heads.remove(draft.package_name());
            return Err(error);
        }
        drop(heads);
        Ok(revision)
    }

    #[doc = "Loads and verifies the newest Registry revision for a package."]
    pub fn latest(
        &self,
        package_name: &PackageName,
    ) -> Result<Option<PackageRevision>, RegistryError> {
        let revisions = self.load_all(package_name)?;
        let latest = revisions.last().cloned();
        let mut heads = self
            .heads
            .lock()
            .map_err(|_| RegistryError::Corrupt("Registry head cache poisoned".to_owned()))?;
        if let Some(revision) = latest.as_ref() {
            let directory = self.revisions_path(package_name);
            let cached = cache::snapshot(&directory, self.owner_uid, Some(revision))?;
            heads.insert(package_name.clone(), cached);
        } else {
            heads.remove(package_name);
        }
        drop(heads);
        Ok(latest)
    }

    #[doc = "Returns the Registry persistence root for fixed-layout discovery."]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn load_all(&self, package_name: &PackageName) -> Result<Vec<PackageRevision>, RegistryError> {
        self.validate_store()?;
        let package = self.package_path(package_name);
        match fs::symlink_metadata(&package) {
            Ok(_) => validate_directory(&package, self.owner_uid)?,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(RegistryError::io(
                    "inspect Registry package",
                    &package,
                    source,
                ));
            }
        }
        let directory = self.revisions_path(package_name);
        validate_directory(&directory, self.owner_uid)?;
        let entries = fs::read_dir(&directory)
            .map_err(|source| RegistryError::io("read package revisions", &directory, source))?;
        let mut files = Vec::new();
        for entry in entries {
            let entry = entry
                .map_err(|source| RegistryError::io("read registry entry", &directory, source))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Err(RegistryError::Corrupt("non-UTF-8 revision file".to_owned()));
            };
            if name.starts_with('.') {
                if valid_temporary_revision_name(name) {
                    store_security::validate_record(
                        &entry.path(),
                        self.owner_uid,
                        "Registry revision",
                    )
                    .map_err(|error| {
                        map_security("validate Registry temporary", &entry.path(), error)
                    })?;
                    continue;
                }
                return Err(RegistryError::Corrupt(format!(
                    "unexpected registry artifact {name}"
                )));
            }
            if !valid_revision_name(name) {
                return Err(RegistryError::Corrupt(format!(
                    "unexpected registry artifact {name}"
                )));
            }
            files.push(entry.path());
        }
        files.sort();
        let mut revisions = Vec::with_capacity(files.len());
        for path in files {
            let bytes = store_security::read_record(&path, self.owner_uid, "Registry revision")
                .map_err(|error| map_security("read Registry revision", &path, error))?;
            let revision = serde_json::from_slice::<PackageRevision>(&bytes).map_err(|source| {
                RegistryError::Corrupt(format!("invalid registry JSON: {source}"))
            })?;
            if revision.package_name() != package_name {
                return Err(RegistryError::Corrupt(
                    "package directory mismatch".to_owned(),
                ));
            }
            revision.verify(revisions.last())?;
            revisions.push(revision);
        }
        Ok(revisions)
    }

    fn revisions_path(&self, package_name: &PackageName) -> PathBuf {
        self.package_path(package_name).join("revisions")
    }

    fn package_path(&self, package_name: &PackageName) -> PathBuf {
        self.packages.join(package_name.as_str())
    }

    fn validate_store(&self) -> Result<(), RegistryError> {
        validate_directory(&self.root, self.owner_uid)?;
        validate_directory(&self.packages, self.owner_uid)
    }
}

pub(super) fn revision_file_name(generation: u64) -> String {
    format!("{generation:016}.json")
}

pub(super) fn valid_revision_name(name: &str) -> bool {
    let Some(prefix) = name.strip_suffix(".json") else {
        return false;
    };
    name.len() == 21 && prefix.bytes().all(|byte| byte.is_ascii_digit())
}

pub(super) fn valid_temporary_revision_name(name: &str) -> bool {
    let Some(name) = name.strip_prefix('.') else {
        return false;
    };
    let Some((revision_name, suffix)) = name.split_once(".tmp-") else {
        return false;
    };
    valid_revision_name(revision_name)
        && suffix.split_once('-').is_some_and(|(process, sequence)| {
            !process.is_empty()
                && !sequence.is_empty()
                && process.bytes().all(|byte| byte.is_ascii_digit())
                && sequence.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn secure_directory(path: &Path, owner_uid: u32) -> Result<(), RegistryError> {
    store_security::ensure_child_directory(path, owner_uid, "Registry")
        .map_err(|error| map_security("create Registry directory", path, error))
}

pub(super) fn validate_directory(path: &Path, owner_uid: u32) -> Result<(), RegistryError> {
    store_security::validate_directory(path, owner_uid, "Registry")
        .map_err(|error| map_security("validate Registry directory", path, error))
}

pub(super) fn map_security(
    action: &'static str,
    path: &Path,
    error: StoreSecurityError,
) -> RegistryError {
    match error {
        StoreSecurityError::Io(source) => RegistryError::io(action, path, source),
        StoreSecurityError::Corrupt(message) => RegistryError::Corrupt(message),
    }
}
