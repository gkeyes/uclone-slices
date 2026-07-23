use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::android::{FileGateLeaseStore, GateLeaseStore};
use crate::domain::PackageName;
use crate::layout::RuntimeLayout;
use crate::service::ServiceError;

use super::stores::ProductionStores;

pub(super) fn discover(stores: &ProductionStores) -> Result<Vec<PackageName>, ServiceError> {
    let mut packages = BTreeMap::<String, PackageName>::new();
    let enrollment = stores
        .enrollment
        .package_names()
        .map_err(|_| ServiceError::RecoveryRequired)?;
    if enrollment.unattributed_corruption() {
        return Err(ServiceError::RecoveryRequired);
    }
    for package in enrollment.package_names() {
        insert(&mut packages, package.clone());
    }
    let journal = stores
        .journal
        .package_names()
        .map_err(|_| ServiceError::RecoveryRequired)?;
    if journal.unattributed_corruption() {
        return Err(ServiceError::RecoveryRequired);
    }
    for package in journal.package_names() {
        insert(&mut packages, package.clone());
    }
    for root in package_roots(stores) {
        scan_root(&root, &mut packages)?;
    }
    let mut leases = FileGateLeaseStore;
    for package in leases
        .package_names()
        .map_err(|_| ServiceError::RecoveryRequired)?
    {
        insert(&mut packages, package);
    }
    Ok(packages.into_values().collect())
}

fn package_roots(stores: &ProductionStores) -> Vec<PathBuf> {
    vec![
        stores.compatibility_policy.root().join("packages"),
        stores.attempts.root().join("attempts"),
        stores.catalog.root().join("packages"),
        stores.package_state.root().join("packages"),
        stores.registry.root().join("packages"),
        stores.slot_metadata.root().join("packages"),
        RuntimeLayout::rescue_journal_root().join("packages"),
        Path::new(crate::target::DE_SLOT_ROOT).to_path_buf(),
    ]
}

fn scan_root(
    root: &Path,
    packages: &mut BTreeMap<String, PackageName>,
) -> Result<(), ServiceError> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(ServiceError::RecoveryRequired),
    };
    for entry in entries {
        let entry = entry.map_err(|_| ServiceError::RecoveryRequired)?;
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_owned)
            .ok_or(ServiceError::RecoveryRequired)?;
        let package = PackageName::parse(&name).map_err(|_| ServiceError::RecoveryRequired)?;
        insert(packages, package);
    }
    Ok(())
}

fn insert(packages: &mut BTreeMap<String, PackageName>, package: PackageName) {
    packages.insert(package.as_str().to_owned(), package);
}
