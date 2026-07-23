use std::path::{Path, PathBuf};
use std::{fs, io};

use super::anchors::RescueAnchors;
use super::{RescueError, RescueJournalStore};
use crate::domain::{PackageKey, UserId};
use crate::journal::JournalStore;
use crate::layout::RuntimeLayout;

#[derive(Debug, Clone)]
pub(super) struct RescueRoots {
    pub(super) management: PathBuf,
    pub(super) enrollment: PathBuf,
    pub(super) catalog: PathBuf,
    pub(super) journal: PathBuf,
}

impl RescueRoots {
    pub(super) fn fixed() -> Self {
        Self {
            management: RuntimeLayout::root().to_path_buf(),
            enrollment: RuntimeLayout::enrollment_root(),
            catalog: RuntimeLayout::catalog_root(),
            journal: RuntimeLayout::rescue_journal_root(),
        }
    }

    pub(super) fn with_roots(
        enrollment: impl AsRef<Path>,
        catalog: impl AsRef<Path>,
        journal: impl AsRef<Path>,
    ) -> Self {
        let enrollment = enrollment.as_ref().to_path_buf();
        let management = enrollment
            .parent()
            .map_or_else(|| enrollment.clone(), Path::to_path_buf);
        Self {
            management,
            enrollment,
            catalog: catalog.as_ref().to_path_buf(),
            journal: journal.as_ref().to_path_buf(),
        }
    }
}

pub(super) fn management_artifacts_present(
    roots: &RescueRoots,
    key: &PackageKey,
) -> Result<bool, crate::rescue::RescueError> {
    if ordinary_journal_mentions_package(roots, key)? {
        return Ok(true);
    }
    let package = key.package_name().as_str();
    let candidates = [
        roots.enrollment.join("packages").join(package),
        roots
            .enrollment
            .join("packages")
            .join(package)
            .join("enrollment.json"),
        roots.catalog.join("packages").join(package),
        roots
            .catalog
            .join("packages")
            .join(package)
            .join("slots/base.json"),
        roots.journal.join("packages").join(package),
        roots
            .management
            .join("registry")
            .join("packages")
            .join(package),
        roots
            .management
            .join("compatibility-policy")
            .join("packages")
            .join(package),
        roots
            .management
            .join("slot-metadata")
            .join("packages")
            .join(package),
        roots
            .management
            .join("package-state")
            .join("packages")
            .join(package),
        roots
            .management
            .join("enrollment-attempts")
            .join("attempts")
            .join(package),
        roots
            .management
            .join("state")
            .join(format!("{package}.gate")),
        roots
            .management
            .join("state")
            .join(format!(".{package}.gate.retiring")),
        roots
            .management
            .join("state")
            .join(format!(".{package}.gate.retired")),
    ];
    for path in candidates {
        match fs::symlink_metadata(&path) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(crate::rescue::RescueError::io(
                    "inspect management artifact",
                    &path,
                    error,
                ));
            }
        }
    }
    Ok(false)
}

pub(super) fn typed_target_evidence(
    roots: &RescueRoots,
    key: &PackageKey,
) -> Result<bool, RescueError> {
    let rescue = match RescueJournalStore::for_package(&roots.journal, key.package_name())
        .and_then(|store| store.load())
    {
        Ok(Some(_)) => true,
        Ok(None) => false,
        Err(error) => {
            if RescueAnchors::load(&roots.enrollment, &roots.catalog, key).is_ok() {
                return Ok(true);
            }
            return Err(error);
        }
    };
    if rescue || RescueAnchors::load(&roots.enrollment, &roots.catalog, key).is_ok() {
        return Ok(true);
    }
    match ordinary_journal_mentions_package(roots, key) {
        Ok(_) => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn ordinary_journal_mentions_package(
    roots: &RescueRoots,
    key: &PackageKey,
) -> Result<bool, crate::rescue::RescueError> {
    let journal_root = roots.management.join("journal");
    let transactions = journal_root.join("transactions");
    match fs::symlink_metadata(&transactions) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(crate::rescue::RescueError::io(
                "inspect ordinary journal transactions",
                &transactions,
                error,
            ));
        }
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(crate::rescue::RescueError::Corrupt(
                "ordinary journal transactions is not a trusted directory".to_owned(),
            ));
        }
        Ok(_) => {}
    }
    let store = JournalStore::new(&journal_root).map_err(|error| {
        crate::rescue::RescueError::Corrupt(format!("ordinary journal validation failed: {error}"))
    })?;
    let transactions = store.list_for_package(key).map_err(|error| {
        crate::rescue::RescueError::Corrupt(format!(
            "ordinary journal package attribution failed: {error}"
        ))
    })?;
    Ok(!transactions.is_empty())
}

pub(super) fn supported(key: &PackageKey) -> bool {
    key.user_id() == UserId::PRIMARY
}

#[cfg(test)]
#[path = "offline_roots/tests.rs"]
mod tests;
