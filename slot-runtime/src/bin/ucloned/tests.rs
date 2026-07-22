#![allow(
    clippy::unwrap_used,
    reason = "daemon startup tests use isolated temporary directories and fail-fast fixtures"
)]

use std::collections::BTreeMap;
use std::fs;

use clap::Parser;
use tempfile::tempdir;

use super::{Cli, management_package_roots, scan_package_root, startup_gate_value};

#[test]
fn accepts_only_the_fixed_startup_gate_flag() {
    assert!(Cli::try_parse_from(["ucloned", "--startup-gate"]).is_ok());
    assert!(Cli::try_parse_from(["ucloned", "--package", "other"]).is_err());
}

#[test]
fn raw_management_scan_retains_recognizable_orphan_package() {
    let root = tempdir().unwrap();
    let package = root.path().join("com.xingin.xhs");
    fs::create_dir(&package).unwrap();
    fs::write(root.path().join("not-a-package"), b"corrupt").unwrap();
    let mut packages = BTreeMap::new();

    let corrupt = scan_package_root(root.path(), &mut packages);

    assert!(corrupt);
    assert!(packages.contains_key("com.xingin.xhs"));
}

#[test]
fn one_held_package_never_masks_another_corrupt_management_artifact() {
    assert!(startup_gate_value(1, true).is_err());
    assert_eq!(startup_gate_value(1, false).unwrap(), "held");
}

#[test]
fn early_management_discovery_includes_device_encrypted_slots() {
    assert!(
        management_package_roots()
            .iter()
            .any(|root| root == std::path::Path::new(uclone_slot_runtime::target::DE_SLOT_ROOT))
    );
}
