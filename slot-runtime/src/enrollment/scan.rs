use std::fs;
use std::path::Path;

use crate::domain::PackageName;
use crate::store_security;

use super::EnrollmentError;

#[doc = "Recognizable enrollment package names plus an aggregate corruption flag."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnrollmentNameScan {
    package_names: Vec<PackageName>,
    corrupt_artifact: bool,
    unattributed_corruption: bool,
}

impl EnrollmentNameScan {
    #[doc = "Returns every recognizable package name in sorted order."]
    pub fn package_names(&self) -> &[PackageName] {
        &self.package_names
    }

    #[doc = "Returns whether any unrecognizable directory artifact was observed."]
    pub const fn corrupt_artifact(&self) -> bool {
        self.corrupt_artifact
    }

    /// Returns whether an invalid artifact had no recognizable package owner.
    pub const fn unattributed_corruption(&self) -> bool {
        self.unattributed_corruption
    }
}

pub(super) fn validate_store_layout(
    root: &Path,
    packages: &Path,
    owner_uid: u32,
) -> Result<(), EnrollmentError> {
    store_security::validate_directory(root, owner_uid, "enrollment")
        .map_err(|error| super::map_security("validate enrollment root", root, error))?;
    let entries = fs::read_dir(root)
        .map_err(|source| EnrollmentError::io("read enrollment root", root, source))?;
    let mut found = false;
    for entry in entries {
        let entry = entry
            .map_err(|source| EnrollmentError::io("read enrollment root artifact", root, source))?;
        let file_type = entry.file_type().map_err(|source| {
            EnrollmentError::io("inspect enrollment root artifact", &entry.path(), source)
        })?;
        if entry.path() != packages || !file_type.is_dir() || found {
            return Err(EnrollmentError::Corrupt(
                "unexpected enrollment root artifact".to_owned(),
            ));
        }
        found = true;
    }
    if found {
        Ok(())
    } else {
        Err(EnrollmentError::Corrupt(
            "missing enrollment packages directory".to_owned(),
        ))
    }
}

pub(super) fn scan_package_names(
    root: &Path,
    owner_uid: u32,
) -> Result<EnrollmentNameScan, EnrollmentError> {
    store_security::validate_directory(root, owner_uid, "enrollment packages")
        .map_err(|error| super::map_security("validate enrollment packages", root, error))?;
    let entries = fs::read_dir(root)
        .map_err(|source| EnrollmentError::io("read enrollment packages", root, source))?;
    let mut package_names = Vec::new();
    let mut corrupt_artifact = false;
    let mut unattributed_corruption = false;
    for entry in entries {
        let Ok(entry) = entry else {
            corrupt_artifact = true;
            unattributed_corruption = true;
            continue;
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            corrupt_artifact = true;
            unattributed_corruption = true;
            continue;
        };
        match PackageName::parse(name) {
            Ok(package_name) => {
                let path = entry.path();
                let is_directory = entry.file_type().is_ok_and(|value| value.is_dir());
                if !is_directory || validate_package_directory(&path, owner_uid).is_err() {
                    corrupt_artifact = true;
                }
                package_names.push(package_name);
            }
            Err(_) => {
                corrupt_artifact = true;
                unattributed_corruption = true;
            }
        }
    }
    package_names.sort();
    package_names.dedup();
    Ok(EnrollmentNameScan {
        package_names,
        corrupt_artifact,
        unattributed_corruption,
    })
}

pub(super) fn validate_package_directory(
    directory: &Path,
    owner_uid: u32,
) -> Result<(), EnrollmentError> {
    let record = inspect_package_directory(directory, owner_uid)?;
    if record.is_none() {
        return Err(EnrollmentError::Corrupt(format!(
            "missing enrollment record in {}",
            directory.display()
        )));
    }
    Ok(())
}

pub(super) fn validate_package_directory_for_create(
    directory: &Path,
    owner_uid: u32,
) -> Result<Option<()>, EnrollmentError> {
    inspect_package_directory(directory, owner_uid)
}

fn inspect_package_directory(
    directory: &Path,
    owner_uid: u32,
) -> Result<Option<()>, EnrollmentError> {
    store_security::validate_directory(directory, owner_uid, "enrollment package")
        .map_err(|error| super::map_security("validate package enrollment", directory, error))?;
    let entries = fs::read_dir(directory)
        .map_err(|source| EnrollmentError::io("read package enrollment", directory, source))?;
    let mut found = false;
    for entry in entries {
        let entry = entry
            .map_err(|source| EnrollmentError::io("read package artifact", directory, source))?;
        let file_type = entry.file_type().map_err(|source| {
            EnrollmentError::io("inspect package artifact", &entry.path(), source)
        })?;
        if entry.file_name() != "enrollment.json" || !file_type.is_file() || found {
            return Err(unexpected(directory));
        }
        store_security::validate_record(&entry.path(), owner_uid, "enrollment record").map_err(
            |error| super::map_security("validate enrollment record", &entry.path(), error),
        )?;
        found = true;
    }
    Ok(found.then_some(()))
}

fn unexpected(directory: &Path) -> EnrollmentError {
    EnrollmentError::Corrupt(format!(
        "unexpected enrollment artifact in {}",
        directory.display()
    ))
}
