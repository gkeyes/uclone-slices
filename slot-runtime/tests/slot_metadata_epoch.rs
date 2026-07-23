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
    SlotDisplayName, SlotMetadataStore, SlotRecordState, SlotSeedMode,
};

fn fixture() -> (TempDir, SlotMetadataStore, PackageName, SlotId) {
    let root = TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let store = SlotMetadataStore::new(root.path().join("metadata")).unwrap();
    (
        root,
        store,
        PackageName::parse("com.example.epoch").unwrap(),
        SlotId::parse("slot-epoch").unwrap(),
    )
}

fn slot_root(root: &TempDir) -> PathBuf {
    root.path()
        .join("metadata/packages/com.example.epoch/slots/slot-epoch")
}

fn collapse_to_legacy_layout(root: &TempDir) {
    let slot = slot_root(root);
    let epochs = slot.join("epochs");
    if !epochs.exists() {
        return;
    }
    let legacy = slot.join("revisions");
    let mut epoch_entries: Vec<_> = fs::read_dir(&epochs)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    epoch_entries.sort();
    for epoch in epoch_entries {
        let mut revisions: Vec<_> = fs::read_dir(epoch.join("revisions"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        revisions.sort();
        for revision in revisions {
            fs::copy(&revision, legacy.join(revision.file_name().unwrap())).unwrap();
        }
    }
    fs::remove_dir_all(epochs).unwrap();
}

fn create_slot(store: &SlotMetadataStore, package: &PackageName, slot: &SlotId) {
    store
        .create(
            package,
            slot.clone(),
            SlotDisplayName::parse("Epoch").unwrap(),
            SlotSeedMode::Blank,
            1,
        )
        .unwrap();
}

fn update_ready(
    store: &SlotMetadataStore,
    package: &PackageName,
    slot: &SlotId,
    opened_version: u64,
) {
    store
        .update(
            package,
            slot,
            SlotDisplayName::parse("Epoch").unwrap(),
            SlotRecordState::Ready,
            opened_version,
        )
        .unwrap();
}

#[test]
fn sixty_four_legacy_revisions_roll_over_before_the_next_update() {
    let (root, store, package, slot) = fixture();
    create_slot(&store, &package, &slot);
    for opened_version in 2..=64 {
        update_ready(&store, &package, &slot, opened_version);
    }
    collapse_to_legacy_layout(&root);
    assert_eq!(
        fs::read_dir(slot_root(&root).join("revisions"))
            .unwrap()
            .count(),
        64
    );

    let next = store
        .update(
            &package,
            &slot,
            SlotDisplayName::parse("Epoch").unwrap(),
            SlotRecordState::Ready,
            65,
        )
        .unwrap();

    assert_eq!(next.generation(), 65);
    assert_eq!(store.latest(&package, &slot).unwrap(), Some(next));
    assert!(
        root.path()
            .join("metadata/packages/com.example.epoch/slots/slot-epoch/epochs")
            .is_dir()
    );
}

#[test]
fn legacy_schema_is_read_without_migration_until_the_next_mutation() {
    let (root, store, package, slot) = fixture();
    create_slot(&store, &package, &slot);

    assert_eq!(
        store.latest(&package, &slot).unwrap().unwrap().generation(),
        1
    );
    assert!(!slot_root(&root).join("epochs").exists());

    update_ready(&store, &package, &slot, 2);

    assert!(slot_root(&root).join("epochs/0000000000000001").is_dir());
    assert!(
        slot_root(&root)
            .join("epochs/0000000000000001/commit.json")
            .is_file()
    );
}

#[test]
fn current_epoch_rolls_at_forty_eight_records_and_keeps_the_old_epoch() {
    let (root, store, package, slot) = fixture();
    create_slot(&store, &package, &slot);
    for opened_version in 2..=49 {
        update_ready(&store, &package, &slot, opened_version);
    }

    update_ready(&store, &package, &slot, 50);

    let slot_root = slot_root(&root);
    assert!(slot_root.join("epochs/0000000000000001").is_dir());
    assert!(slot_root.join("epochs/0000000000000002").is_dir());
    assert!(
        slot_root
            .join("epochs/0000000000000002/commit.json")
            .is_file()
    );
    assert_eq!(
        store.latest(&package, &slot).unwrap().unwrap().generation(),
        50
    );
}

#[test]
fn incomplete_epoch_publication_fails_closed() {
    let (root, store, package, slot) = fixture();
    create_slot(&store, &package, &slot);
    let epochs = slot_root(&root).join("epochs");
    fs::create_dir(&epochs).unwrap();
    fs::set_permissions(&epochs, fs::Permissions::from_mode(0o700)).unwrap();

    assert!(store.latest(&package, &slot).is_err());
}

#[test]
fn unrenamed_epoch_temporary_is_rejected_without_replacing_the_old_head() {
    let (root, store, package, slot) = fixture();
    create_slot(&store, &package, &slot);
    update_ready(&store, &package, &slot, 2);
    let epochs = slot_root(&root).join("epochs");
    let temporary = epochs.join(".0000000000000002.tmp-crash");
    fs::create_dir(&temporary).unwrap();
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o700)).unwrap();

    assert!(store.latest(&package, &slot).is_err());
    fs::remove_dir(&temporary).unwrap();
    assert_eq!(
        store.latest(&package, &slot).unwrap().unwrap().generation(),
        2
    );
}

#[test]
fn checkpoint_corruption_and_epoch_chain_breaks_fail_closed() {
    let (root, store, package, slot) = fixture();
    create_slot(&store, &package, &slot);
    update_ready(&store, &package, &slot, 2);
    let checkpoint = slot_root(&root).join("epochs/0000000000000001/checkpoint.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&checkpoint).unwrap()).unwrap();
    value["previous_record_count"] = serde_json::json!(99);
    fs::write(&checkpoint, serde_json::to_vec(&value).unwrap()).unwrap();

    assert!(store.latest(&package, &slot).is_err());
}

#[test]
fn competing_epoch_and_commit_rollback_are_rejected() {
    for corrupt in ["extra_epoch", "missing_commit"] {
        let (root, store, package, slot) = fixture();
        create_slot(&store, &package, &slot);
        for opened_version in 2..=50 {
            update_ready(&store, &package, &slot, opened_version);
        }
        let slot_root = slot_root(&root);
        if corrupt == "extra_epoch" {
            let extra = slot_root.join("epochs/0000000000000003");
            fs::create_dir(&extra).unwrap();
            fs::set_permissions(&extra, fs::Permissions::from_mode(0o700)).unwrap();
        } else {
            fs::remove_file(slot_root.join("epochs/0000000000000002/commit.json")).unwrap();
        }

        assert!(store.latest(&package, &slot).is_err(), "{corrupt}");
    }
}
