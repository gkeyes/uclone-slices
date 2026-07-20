#![doc = "Slot catalog creation-invariant tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

use tempfile::TempDir;
use uclone_slot_runtime::catalog::{
    CatalogError, CatalogStore, PathSecurityProof, SecurityProfileProof,
};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, PackageKey, PackageName, SlotId, UserId,
};

const SIGNATURE_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SIGNATURE_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn key() -> PackageKey {
    PackageKey::new(
        PackageName::parse("com.uclone.slotprobe").unwrap(),
        UserId::PRIMARY,
    )
}

fn identity(signature: &str, version: u64) -> AppIdentity {
    AppIdentity::new(10_321, signature, version, "/data/app/slotprobe/base.apk").unwrap()
}

fn proof(uid: u32) -> SecurityProfileProof {
    let path = PathSecurityProof::new(
        uid,
        uid,
        0o700,
        "u:object_r:app_data_file:s0:c1,c2",
        SIGNATURE_A,
    )
    .unwrap();
    SecurityProfileProof::new(path.clone(), path)
}

fn fixture() -> (TempDir, CatalogStore, PackageKey, AppIdentity) {
    let root = TempDir::new().unwrap();
    let store = CatalogStore::new(root.path()).unwrap();
    let key = key();
    let identity = identity(SIGNATURE_A, 1);
    store
        .create_base(
            key.clone(),
            DataInodes::new(100, 200).unwrap(),
            identity.clone(),
            proof(10_321),
        )
        .unwrap();
    (root, store, key, identity)
}

#[test]
fn rejects_duplicate_base_and_slot_without_overwrite() {
    let (_root, store, key, enrolled) = fixture();
    let base_error = store
        .create_base(
            key.clone(),
            DataInodes::new(101, 201).unwrap(),
            enrolled.clone(),
            proof(10_321),
        )
        .unwrap_err();
    assert!(matches!(base_error, CatalogError::DuplicateSlot { .. }));

    let slot = SlotId::parse("work").unwrap();
    store
        .append_slot(
            &key,
            slot.clone(),
            DataInodes::new(300, 400).unwrap(),
            &enrolled,
            1,
            proof(10_321),
        )
        .unwrap();
    let duplicate = store
        .append_slot(
            &key,
            slot,
            DataInodes::new(500, 600).unwrap(),
            &enrolled,
            1,
            proof(10_321),
        )
        .unwrap_err();

    assert!(matches!(duplicate, CatalogError::DuplicateSlot { .. }));
    assert_eq!(
        store.list(&key).unwrap().get(1).unwrap().inodes(),
        DataInodes::new(300, 400).unwrap()
    );
}

#[test]
fn rejects_either_base_inode_reused_by_non_base_slot() {
    for inodes in [
        DataInodes::new(100, 400).unwrap(),
        DataInodes::new(300, 200).unwrap(),
    ] {
        let (_root, store, key, enrolled) = fixture();

        let error = store
            .append_slot(
                &key,
                SlotId::parse("work").unwrap(),
                inodes,
                &enrolled,
                1,
                proof(10_321),
            )
            .unwrap_err();

        assert!(matches!(error, CatalogError::BaseInodeReuse { .. }));
    }
}

#[test]
fn rejects_enrollment_identity_mismatch() {
    let (_root, store, key, _) = fixture();
    let changed = identity(SIGNATURE_B, 2);

    let error = store
        .append_slot(
            &key,
            SlotId::parse("work").unwrap(),
            DataInodes::new(300, 400).unwrap(),
            &changed,
            2,
            proof(10_321),
        )
        .unwrap_err();

    assert!(matches!(error, CatalogError::IdentityMismatch(_)));
}

#[test]
fn rejects_security_owner_mismatch_and_invalid_proof_fields() {
    let root = TempDir::new().unwrap();
    let store = CatalogStore::new(root.path()).unwrap();
    let invalid_owner = store
        .create_base(
            key(),
            DataInodes::new(100, 200).unwrap(),
            identity(SIGNATURE_A, 1),
            proof(10_999),
        )
        .unwrap_err();
    assert!(matches!(invalid_owner, CatalogError::Invalid(_)));

    assert!(
        PathSecurityProof::new(
            10_321,
            10_321,
            0o10_000,
            "u:object_r:app_data_file:s0",
            SIGNATURE_A,
        )
        .is_err()
    );
    assert!(PathSecurityProof::new(10_321, 10_321, 0o700, "", SIGNATURE_A).is_err());
    assert!(
        PathSecurityProof::new(
            10_321,
            10_321,
            0o700,
            "u:object_r:app_data_file:s0",
            "not-a-digest",
        )
        .is_err()
    );
}

#[test]
fn append_requires_an_existing_base_catalog() {
    let root = TempDir::new().unwrap();
    let store = CatalogStore::new(root.path()).unwrap();
    let package = key();
    let enrolled = identity(SIGNATURE_A, 1);

    let error = store
        .append_slot(
            &package,
            SlotId::parse("work").unwrap(),
            DataInodes::new(300, 400).unwrap(),
            &enrolled,
            1,
            proof(10_321),
        )
        .unwrap_err();

    assert!(matches!(error, CatalogError::MissingBase(_)));
}
