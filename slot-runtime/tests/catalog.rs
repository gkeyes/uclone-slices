#![doc = "Slot catalog creation, lookup, and path-derivation tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

use std::fs;
use std::path::Path;

use tempfile::TempDir;
use uclone_slot_runtime::catalog::{CatalogStore, PathSecurityProof, SecurityProfileProof};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, PackageKey, PackageName, SlotId, UserId,
};

const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CE_POLICY: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const DE_POLICY: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn key(package: &str) -> PackageKey {
    PackageKey::new(PackageName::parse(package).unwrap(), UserId::PRIMARY)
}

fn identity(version: u64) -> AppIdentity {
    AppIdentity::new(10_321, SIGNATURE, version, "/data/app/slotprobe/base.apk").unwrap()
}

fn proof() -> SecurityProfileProof {
    SecurityProfileProof::new(
        PathSecurityProof::new(
            10_321,
            10_321,
            0o700,
            "u:object_r:app_data_file:s0:c1,c2",
            CE_POLICY,
        )
        .unwrap(),
        PathSecurityProof::new(
            10_321,
            10_321,
            0o700,
            "u:object_r:app_data_file:s0:c1,c2",
            DE_POLICY,
        )
        .unwrap(),
    )
}

#[test]
fn creates_base_and_append_only_slot_with_derived_paths() {
    let root = TempDir::new().unwrap();
    let store = CatalogStore::new(root.path()).unwrap();
    let package = key("com.uclone.slotprobe");
    let enrolled = identity(7);
    let base = DataInodes::new(100, 200).unwrap();

    let base_entry = store
        .create_base(package.clone(), base, enrolled.clone(), proof())
        .unwrap();
    let work_entry = store
        .append_slot(
            &package,
            SlotId::parse("work").unwrap(),
            DataInodes::new(300, 400).unwrap(),
            &enrolled,
            9,
            proof(),
        )
        .unwrap();

    assert!(base_entry.slot_id().is_base());
    assert_eq!(base_entry.created_version_code(), 7);
    assert_eq!(work_entry.created_version_code(), 9);
    assert_eq!(work_entry.package_key(), &package);
    assert_eq!(work_entry.enrolled_identity(), &enrolled);
    assert_eq!(work_entry.inodes(), DataInodes::new(300, 400).unwrap());
    assert_eq!(
        base_entry.source_paths().ce(),
        Path::new("/data/user/0/com.uclone.slotprobe")
    );
    assert_eq!(
        work_entry.source_paths().de(),
        Path::new("/data/misc_de/0/uclone-slices-preview/slots/com.uclone.slotprobe/work")
    );

    let listed = store.list(&package).unwrap();
    assert_eq!(listed, vec![base_entry, work_entry.clone()]);
    assert_eq!(
        store
            .lookup(&package, &SlotId::parse("work").unwrap())
            .unwrap(),
        Some(work_entry)
    );
    assert_eq!(store.enumerate().unwrap(), listed);
}

#[test]
fn persisted_manifest_never_contains_slot_source_paths() {
    let root = TempDir::new().unwrap();
    let store = CatalogStore::new(root.path()).unwrap();
    let package = key("com.uclone.slotprobe");
    let enrolled = identity(7);
    store
        .create_base(
            package.clone(),
            DataInodes::new(100, 200).unwrap(),
            enrolled.clone(),
            proof(),
        )
        .unwrap();
    store
        .append_slot(
            &package,
            SlotId::parse("work").unwrap(),
            DataInodes::new(300, 400).unwrap(),
            &enrolled,
            7,
            proof(),
        )
        .unwrap();

    let bytes = fs::read(
        root.path()
            .join("packages/com.uclone.slotprobe/slots/work.json"),
    )
    .unwrap();
    let json = String::from_utf8(bytes).unwrap();

    assert!(!json.contains("source_path"));
    assert!(!json.contains("/data/misc_ce/0/uclone-slices-preview/slots"));
    assert!(!json.contains("/data/misc_de/0/uclone-slices-preview/slots"));
    assert!(json.contains("\"sha256\""));
    assert!(json.contains("\"schema_version\":1"));
}

#[test]
fn enumerates_multiple_user_zero_package_catalogs_in_stable_order() {
    let root = TempDir::new().unwrap();
    let store = CatalogStore::new(root.path()).unwrap();
    let enrolled = identity(1);
    for package in ["org.example.beta", "com.example.alpha"] {
        store
            .create_base(
                key(package),
                DataInodes::new(100, 200).unwrap(),
                enrolled.clone(),
                proof(),
            )
            .unwrap();
    }

    let entries = store.enumerate().unwrap();

    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries
            .first()
            .unwrap()
            .package_key()
            .package_name()
            .as_str(),
        "com.example.alpha"
    );
    assert_eq!(
        entries
            .get(1)
            .unwrap()
            .package_key()
            .package_name()
            .as_str(),
        "org.example.beta"
    );
}
