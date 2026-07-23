use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use uclone_slot_runtime::android::{FileGateLeaseStore, GateLeaseStore};
use uclone_slot_runtime::domain::{PackageKey, PackageName, UserId};
use uclone_slot_runtime::enrollment::EnrollmentStore;
use uclone_slot_runtime::journal::JournalStore;
use uclone_slot_runtime::layout::RuntimeLayout;

pub fn startup_keys() -> (Vec<PackageKey>, bool) {
    let mut packages = BTreeMap::<String, PackageName>::new();
    let mut corrupt = false;
    for root in management_package_roots() {
        corrupt |= scan_package_root(&root, &mut packages);
    }
    match EnrollmentStore::new(RuntimeLayout::enrollment_root())
        .and_then(|store| store.package_names())
    {
        Ok(scan) => {
            corrupt |= scan.unattributed_corruption();
            for package in scan.package_names() {
                insert_package(&mut packages, package.clone());
            }
        }
        Err(_) => corrupt = true,
    }
    match JournalStore::new(RuntimeLayout::journal_root()).and_then(|store| store.package_names()) {
        Ok(scan) => {
            corrupt |= scan.unattributed_corruption();
            for package in scan.package_names() {
                insert_package(&mut packages, package.clone());
            }
        }
        Err(_) => corrupt = true,
    }
    let mut leases = FileGateLeaseStore;
    match leases.package_names() {
        Ok(packages_with_leases) => {
            for package in packages_with_leases {
                insert_package(&mut packages, package);
            }
        }
        Err(_) => corrupt = true,
    }
    (
        packages
            .into_values()
            .map(|package| PackageKey::new(package, UserId::PRIMARY))
            .collect(),
        corrupt,
    )
}

pub fn management_package_roots() -> Vec<PathBuf> {
    vec![
        RuntimeLayout::enrollment_root().join("packages"),
        RuntimeLayout::compatibility_policy_root().join("packages"),
        RuntimeLayout::catalog_root().join("packages"),
        RuntimeLayout::registry_root().join("packages"),
        RuntimeLayout::package_state_root().join("packages"),
        RuntimeLayout::slot_metadata_root().join("packages"),
        RuntimeLayout::enrollment_attempt_root().join("attempts"),
        RuntimeLayout::rescue_journal_root().join("packages"),
        Path::new(uclone_slot_runtime::target::DE_SLOT_ROOT).to_path_buf(),
    ]
}

pub fn scan_package_root(root: &Path, packages: &mut BTreeMap<String, PackageName>) -> bool {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(_) => return true,
    };
    let mut corrupt = false;
    for entry in entries {
        let Ok(entry) = entry else {
            corrupt = true;
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            corrupt = true;
            continue;
        };
        match PackageName::parse(&name) {
            Ok(package) => {
                insert_package(packages, package);
            }
            Err(_) => corrupt = true,
        }
    }
    corrupt
}

fn insert_package(packages: &mut BTreeMap<String, PackageName>, package: PackageName) {
    packages.insert(package.as_str().to_owned(), package);
}

pub fn reconciliation_keys(
    ordinary: &[PackageKey],
    recovery: &BTreeSet<PackageName>,
) -> Vec<PackageKey> {
    recovery
        .iter()
        .cloned()
        .map(|package| PackageKey::new(package, UserId::PRIMARY))
        .chain(ordinary.iter().cloned())
        .collect()
}
