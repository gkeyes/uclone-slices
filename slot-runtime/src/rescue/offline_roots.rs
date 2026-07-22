use std::path::{Path, PathBuf};
use std::{fs, io};

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
    if ordinary_journal_mentions_package(roots, key) {
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

fn ordinary_journal_mentions_package(roots: &RescueRoots, key: &PackageKey) -> bool {
    let journal_root = roots.management.join("journal");
    let transactions = journal_root.join("transactions");
    match fs::symlink_metadata(&transactions) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return false,
        Err(_) => return true,
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => return true,
        Ok(_) => {}
    }
    let Ok(store) = JournalStore::new(&journal_root) else {
        return true;
    };
    let Ok(transactions) = store.list() else {
        return true;
    };
    transactions.iter().any(|transaction| {
        transaction.spec().package_name() == key.package_name()
            && transaction.spec().user_id() == key.user_id()
    })
}

pub(super) fn supported(key: &PackageKey) -> bool {
    key.user_id() == UserId::PRIMARY
}

#[cfg(test)]
#[path = "offline_roots/tests.rs"]
mod tests;
