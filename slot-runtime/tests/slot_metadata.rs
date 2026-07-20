#![allow(
    missing_docs,
    clippy::unwrap_used,
    reason = "isolated append-only metadata fixtures abort the invoking test"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{PackageName, SlotId};
use uclone_slot_runtime::slot_metadata::{
    SlotDisplayName, SlotMetadataError, SlotMetadataStore, SlotRecordState, SlotSeedMode,
};

fn package(value: &str) -> PackageName {
    PackageName::parse(value).unwrap()
}

fn slot(value: &str) -> SlotId {
    SlotId::parse(value).unwrap()
}

fn fixture() -> (TempDir, SlotMetadataStore) {
    let root = TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let store = SlotMetadataStore::new(root.path().join("metadata")).unwrap();
    (root, store)
}

fn revision(root: &TempDir, package: &str, slot: &str, generation: u64) -> PathBuf {
    root.path()
        .join("metadata/packages")
        .join(package)
        .join("slots")
        .join(slot)
        .join("revisions")
        .join(format!("{generation:016}.json"))
}

#[test]
fn display_names_never_become_path_identifiers() {
    for valid in ["工作", "Personal profile", "A/B"] {
        assert_eq!(SlotDisplayName::parse(valid).unwrap().as_str(), valid);
    }
    for invalid in ["", " leading", "trailing ", "line\nbreak"] {
        assert!(SlotDisplayName::parse(invalid).is_err());
    }
    assert!(SlotId::parse("../escape").is_err());
    assert!(PackageName::parse("../../data").is_err());
}

#[test]
fn package_local_stream_preserves_immutable_slot_identity_and_seed() {
    let (_root, store) = fixture();
    let first = package("com.example.first");
    let second = package("com.example.second");
    let slot = slot("slot-1");
    store
        .create(
            &first,
            slot.clone(),
            SlotDisplayName::parse("Blank").unwrap(),
            SlotSeedMode::Blank,
            7,
        )
        .unwrap();
    store
        .create(
            &second,
            slot.clone(),
            SlotDisplayName::parse("Copy").unwrap(),
            SlotSeedMode::CloneBase,
            11,
        )
        .unwrap();
    let ready = store
        .update(
            &first,
            &slot,
            SlotDisplayName::parse("Fresh").unwrap(),
            SlotRecordState::Ready,
            8,
        )
        .unwrap();

    assert_eq!(ready.generation(), 2);
    assert_eq!(ready.seed_mode(), SlotSeedMode::Blank);
    assert_eq!(store.list(&first).unwrap().len(), 1);
    assert_eq!(store.list(&second).unwrap().len(), 1);
    assert_eq!(
        store
            .latest(&second, &slot)
            .unwrap()
            .unwrap()
            .created_version_code(),
        11
    );
}

#[test]
fn deleted_stream_is_terminal_and_base_is_never_a_metadata_slot() {
    let (_root, store) = fixture();
    let package = package("com.example.app");
    assert!(matches!(
        store.create(
            &package,
            SlotId::base(),
            SlotDisplayName::parse("Base").unwrap(),
            SlotSeedMode::CloneBase,
            1,
        ),
        Err(SlotMetadataError::Invalid(_))
    ));
    let slot = slot("slot-2");
    store
        .create(
            &package,
            slot.clone(),
            SlotDisplayName::parse("Temporary").unwrap(),
            SlotSeedMode::Blank,
            1,
        )
        .unwrap();
    store
        .update(
            &package,
            &slot,
            SlotDisplayName::parse("Temporary").unwrap(),
            SlotRecordState::Deleted,
            1,
        )
        .unwrap();
    assert!(matches!(
        store.update(
            &package,
            &slot,
            SlotDisplayName::parse("Revived").unwrap(),
            SlotRecordState::Ready,
            1,
        ),
        Err(SlotMetadataError::Invalid(_))
    ));
}

#[test]
fn schema_identity_and_digest_tampering_fail_closed() {
    for field in ["schema_version", "package", "sha256"] {
        let (root, store) = fixture();
        let package = package("com.example.app");
        let slot = slot("slot-3");
        store
            .create(
                &package,
                slot.clone(),
                SlotDisplayName::parse("Work").unwrap(),
                SlotSeedMode::Blank,
                3,
            )
            .unwrap();
        let path = revision(&root, package.as_str(), slot.as_str(), 1);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let replacement = match field {
            "schema_version" => serde_json::json!(99),
            "package" => serde_json::json!("com.example.other"),
            "sha256" => serde_json::json!("00"),
            _ => unreachable!(),
        };
        *value.get_mut(field).unwrap() = replacement;
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(store.latest(&package, &slot).is_err());
    }
}
