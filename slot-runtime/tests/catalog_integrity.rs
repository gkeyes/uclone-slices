#![doc = "Slot catalog persistence-integrity and strict-artifact tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

use std::fs;

use tempfile::TempDir;
use uclone_slot_runtime::catalog::{
    CatalogError, CatalogStore, PathSecurityProof, SecurityProfileProof,
};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, PackageKey, PackageName, SlotId, UserId,
};

const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn fixture() -> (TempDir, CatalogStore, PackageKey, AppIdentity) {
    let root = TempDir::new().unwrap();
    let store = CatalogStore::new(root.path()).unwrap();
    let key = PackageKey::new(
        PackageName::parse("com.uclone.slotprobe").unwrap(),
        UserId::PRIMARY,
    );
    let identity = AppIdentity::new(10_321, DIGEST, 1, "/data/app/slotprobe/base.apk").unwrap();
    store
        .create_base(
            key.clone(),
            DataInodes::new(100, 200).unwrap(),
            identity.clone(),
            proof(),
        )
        .unwrap();
    (root, store, key, identity)
}

fn proof() -> SecurityProfileProof {
    let path = PathSecurityProof::new(
        10_321,
        10_321,
        0o700,
        "u:object_r:app_data_file:s0:c1,c2",
        DIGEST,
    )
    .unwrap();
    SecurityProfileProof::new(path.clone(), path)
}

fn add_work(store: &CatalogStore, key: &PackageKey, identity: &AppIdentity) {
    store
        .append_slot(
            key,
            SlotId::parse("work").unwrap(),
            DataInodes::new(300, 400).unwrap(),
            identity,
            1,
            proof(),
        )
        .unwrap();
}

#[test]
fn rejects_hash_tampering() {
    let (root, store, key, identity) = fixture();
    add_work(&store, &key, &identity);
    let path = root
        .path()
        .join("packages/com.uclone.slotprobe/slots/work.json");
    let json = fs::read_to_string(&path).unwrap();
    fs::write(
        &path,
        json.replace("\"created_version_code\":1", "\"created_version_code\":2"),
    )
    .unwrap();

    let error = store
        .lookup(&key, &SlotId::parse("work").unwrap())
        .unwrap_err();

    assert!(matches!(error, CatalogError::Corrupt(_)));
    assert!(error.to_string().contains("digest mismatch"));
}

#[test]
fn rejects_unknown_root_and_nested_fields() {
    for injection in ["{\"rogue\":true,", "\"security_profile\":{\"rogue\":true,"] {
        let (root, store, key, _) = fixture();
        let path = root
            .path()
            .join("packages/com.uclone.slotprobe/slots/base.json");
        let json = fs::read_to_string(&path).unwrap();
        let modified = if injection.starts_with('{') {
            json.replacen('{', injection, 1)
        } else {
            json.replacen("\"security_profile\":{", injection, 1)
        };
        fs::write(path, modified).unwrap();

        let error = store.list(&key).unwrap_err();

        assert!(matches!(error, CatalogError::Corrupt(_)));
        assert!(error.to_string().contains("unknown field"));
    }
}

#[test]
fn rejects_filename_mismatch() {
    let (root, store, key, identity) = fixture();
    add_work(&store, &key, &identity);
    let slots = root.path().join("packages/com.uclone.slotprobe/slots");
    fs::rename(slots.join("work.json"), slots.join("renamed.json")).unwrap();

    let error = store.list(&key).unwrap_err();

    assert!(matches!(error, CatalogError::Corrupt(_)));
    assert!(error.to_string().contains("filename mismatch"));
}

#[test]
fn rejects_unexpected_package_and_slot_artifacts() {
    let (root, store, key, _) = fixture();
    let package = root.path().join("packages/com.uclone.slotprobe");
    fs::write(package.join("notes.txt"), b"untrusted").unwrap();

    let error = store.list(&key).unwrap_err();
    assert!(matches!(error, CatalogError::Corrupt(_)));
    assert!(error.to_string().contains("unexpected catalog artifact"));

    fs::remove_file(package.join("notes.txt")).unwrap();
    fs::write(package.join("slots/notes.txt"), b"untrusted").unwrap();
    let error = store.list(&key).unwrap_err();
    assert!(matches!(error, CatalogError::Corrupt(_)));
    assert!(error.to_string().contains("unexpected slot artifact"));
}

#[test]
fn rejects_package_directory_mismatch_during_enumeration() {
    let (root, store, _, _) = fixture();
    let packages = root.path().join("packages");
    fs::rename(
        packages.join("com.uclone.slotprobe"),
        packages.join("com.example.renamed"),
    )
    .unwrap();

    let error = store.enumerate().unwrap_err();

    assert!(matches!(error, CatalogError::Corrupt(_)));
    assert!(error.to_string().contains("package directory mismatch"));
}
