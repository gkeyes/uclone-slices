#![allow(clippy::unwrap_used)]

use super::*;
use crate::model::{DisplayName, PackageAggregate, PackageIdentity, RestorePath};
use std::os::unix::fs::symlink;

fn identity() -> PackageIdentity {
    PackageIdentity::new(10_000, "/data/app/example/base.apk", 1, 2).unwrap()
}

fn resource_preserve_policy() -> RestorePolicy {
    RestorePolicy::PreservePaths {
        paths: vec![
            RestorePath {
                domain: RestoreDomain::Ce,
                path: "files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer".to_owned(),
            },
            RestorePath {
                domain: RestoreDomain::Ce,
                path: "files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/*/Paks".to_owned(),
            },
        ],
    }
}

fn open_slot_storage(
    ce_slots: &Path,
    de_slots: &Path,
    canonical_ce: &Path,
    canonical_de: &Path,
) -> FileSlotStorage {
    let existing_ancestor = ce_slots.ancestors().find(|path| path.exists()).unwrap();
    let metadata = fs::metadata(existing_ancestor).unwrap();
    FileSlotStorage::open_with_owner(
        ce_slots,
        de_slots,
        canonical_ce,
        canonical_de,
        DirectoryOwner::new(metadata.uid(), metadata.gid()),
    )
    .unwrap()
}

#[test]
fn legacy_base_alias_mount_detection_counts_every_stacked_layer() {
    let mountinfo = concat!(
        "41 30 0:35 / /data/misc_ce/0/uclone-slices-v2/maintenance-base/token/pkg rw - ext4 /dev/block/data rw\n",
        "42 41 0:35 /maintenance/token/pkg /data/misc_ce/0/uclone-slices-v2/maintenance-base/token/pkg rw - ext4 /dev/block/data rw\n",
        "43 30 0:35 / /data/misc_de/0/uclone-slices-v2/maintenance-base/token/pkg rw - ext4 /dev/block/data rw\n",
    );
    assert_eq!(
        mountinfo_path_layer_count(
            mountinfo,
            Path::new("/data/misc_ce/0/uclone-slices-v2/maintenance-base/token/pkg"),
        ),
        2
    );
    assert_eq!(
        mountinfo_path_layer_count(
            mountinfo,
            Path::new("/data/misc_ce/0/uclone-slices-v2/maintenance-base/token"),
        ),
        0
    );
    assert_eq!(
        mountinfo_path_layer_count(
            mountinfo,
            Path::new("/data/misc_de/0/uclone-slices-v2/maintenance-base/token/pkg"),
        ),
        1
    );
}

#[test]
fn legacy_base_alias_cleanup_unmounts_every_stacked_layer() {
    let target = Path::new("/legacy-alias");
    let mut layers = std::collections::VecDeque::from([3, 2, 1, 0]);
    let mut unmounts = 0;

    drain_mount_layers(
        target,
        |_path| Ok(layers.pop_front().unwrap()),
        |_path| {
            unmounts += 1;
            Ok(())
        },
    )
    .unwrap();

    assert_eq!(unmounts, 3);
    assert!(layers.is_empty());
}

#[test]
fn restore_maintenance_does_not_create_a_base_alias() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let canonical_ce_root = root.path().join("canonical-ce");
    let canonical_de_root = root.path().join("canonical-de");
    fs::create_dir_all(canonical_ce_root.join(package.as_str())).unwrap();
    fs::create_dir_all(canonical_de_root.join(package.as_str())).unwrap();
    fs::create_dir(root.path().join("ce")).unwrap();
    fs::create_dir(root.path().join("de")).unwrap();
    let mut storage = open_slot_storage(
        &root.path().join("ce/uclone-slices-v2/slots"),
        &root.path().join("de/uclone-slices-v2/slots"),
        &canonical_ce_root,
        &canonical_de_root,
    );
    let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();

    storage.prepare_maintenance(&package, &token).unwrap();

    let (maintenance_ce, maintenance_de) = storage.maintenance_paths(&package, &token).unwrap();
    let (alias_ce, alias_de) = storage.base_alias_paths(&package, &token).unwrap();
    assert!(maintenance_ce.is_dir());
    assert!(maintenance_de.is_dir());
    assert!(!alias_ce.exists());
    assert!(!alias_de.exists());
}

#[test]
fn aggregate_round_trips_atomically() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let mut store = FilePackageStore::open(root.path()).unwrap();
    let aggregate = PackageAggregate::enrolled(package.clone(), identity());

    store.save(&aggregate).unwrap();

    assert_eq!(store.load(&package).unwrap(), Some(aggregate));
    assert_eq!(store.list().unwrap(), vec![package]);
}

#[test]
fn pre_v1_binding_backup_records_aggregate_and_both_domain_inventories_once() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let mut store = FilePackageStore::open(root.path()).unwrap();
    let aggregate = PackageAggregate::enrolled(package.clone(), identity());
    let pairs = vec![SlotId::numbered(1), SlotId::numbered(2)];

    store.backup_before_v1_binding(&aggregate, &pairs).unwrap();

    let backup_root = root
        .path()
        .join("state-backups/pre-0.1.7")
        .join(package.as_str());
    let restored: PackageAggregate =
        serde_json::from_slice(&fs::read(backup_root.join("aggregate.json")).unwrap()).unwrap();
    let inventory: SlotPairBackup =
        serde_json::from_slice(&fs::read(backup_root.join("slot-pairs.json")).unwrap()).unwrap();
    assert_eq!(restored, aggregate);
    assert_eq!(inventory.ce, pairs);
    assert_eq!(inventory.de, pairs);
}

#[test]
fn aggregate_removal_is_idempotent_and_does_not_touch_other_packages() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let other = PackageName::new("com.example.other").unwrap();
    let mut store = FilePackageStore::open(root.path()).unwrap();
    store
        .save(&PackageAggregate::enrolled(package.clone(), identity()))
        .unwrap();
    store
        .save(&PackageAggregate::enrolled(other.clone(), identity()))
        .unwrap();

    store.remove(&package).unwrap();
    store.remove(&package).unwrap();

    assert!(store.load(&package).unwrap().is_none());
    assert!(store.load(&other).unwrap().is_some());
    assert_eq!(store.list().unwrap(), vec![other]);
}

#[test]
fn package_directory_without_an_aggregate_is_not_listed() {
    let root = tempfile::tempdir().unwrap();
    let store = FilePackageStore::open(root.path()).unwrap();
    fs::create_dir_all(root.path().join("packages/com.example.partial")).unwrap();

    assert!(store.list().unwrap().is_empty());
}

#[test]
fn base_clone_materializes_both_domains() {
    let root = tempfile::tempdir().unwrap();
    let ce = root.path().join("base-ce");
    let de = root.path().join("base-de");
    let package = PackageName::new("com.example.app").unwrap();
    fs::create_dir_all(ce.join(package.as_str())).unwrap();
    fs::create_dir_all(de.join(package.as_str())).unwrap();
    fs::write(ce.join(package.as_str()).join("ce.txt"), b"ce").unwrap();
    fs::write(de.join(package.as_str()).join("de.txt"), b"de").unwrap();
    let ce_slots = root.path().join("slots-ce");
    let de_slots = root.path().join("slots-de");
    let mut storage = open_slot_storage(&ce_slots, &de_slots, &ce, &de);
    let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
    let slot = aggregate
        .reserve_slot(DisplayName::new("Clone").unwrap())
        .unwrap();

    storage
        .materialize(&package, &slot, SeedMode::CloneBase)
        .unwrap();

    let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
    assert_eq!(fs::read(target_ce.join("ce.txt")).unwrap(), b"ce");
    assert_eq!(fs::read(target_de.join("de.txt")).unwrap(), b"de");
    assert_eq!(
        fs::metadata(target_ce.parent().unwrap()).unwrap().mode() & 0o777,
        0o700,
    );
    assert_eq!(
        fs::metadata(target_de.parent().unwrap()).unwrap().mode() & 0o777,
        0o700,
    );
}

#[test]
fn base_clone_capacity_counts_the_base_tree_and_blank_needs_none() {
    let root = tempfile::tempdir().unwrap();
    let ce = root.path().join("base-ce");
    let de = root.path().join("base-de");
    let package = PackageName::new("com.example.app").unwrap();
    fs::create_dir_all(ce.join(package.as_str())).unwrap();
    fs::create_dir_all(de.join(package.as_str())).unwrap();
    fs::write(
        ce.join(package.as_str()).join("ce.bin"),
        vec![1_u8; 64 * 1024],
    )
    .unwrap();
    let ce_slots = root.path().join("slots-ce");
    let de_slots = root.path().join("slots-de");
    let mut storage = open_slot_storage(&ce_slots, &de_slots, &ce, &de);
    let required = copied_tree_bytes(&ce.join(package.as_str())).unwrap()
        + copied_tree_bytes(&de.join(package.as_str())).unwrap();

    storage
        .set_available_bytes(&ce_slots, required - 1)
        .unwrap();
    let error = storage
        .ensure_materialize_capacity(&package, SeedMode::CloneBase)
        .unwrap_err();
    assert!(error.is_insufficient_storage());
    assert!(
        storage
            .ensure_materialize_capacity(&package, SeedMode::Blank)
            .is_ok()
    );

    storage.set_available_bytes(&ce_slots, required).unwrap();
    assert!(
        storage
            .ensure_materialize_capacity(&package, SeedMode::CloneBase)
            .is_ok()
    );
}

#[test]
fn package_listing_skips_a_foreign_directory() {
    let root = tempfile::tempdir().unwrap();
    let store = FilePackageStore::open(root.path()).unwrap();
    let mut writable = FilePackageStore::open(root.path()).unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    writable
        .save(&PackageAggregate::enrolled(package.clone(), identity()))
        .unwrap();
    let foreign = root.path().join("packages").join("Not A Package");
    fs::create_dir_all(&foreign).unwrap();
    fs::write(foreign.join("aggregate.json"), b"{}").unwrap();

    assert_eq!(store.list().unwrap(), vec![package]);
}

#[test]
fn blank_slot_inherits_both_base_root_modes() {
    let root = tempfile::tempdir().unwrap();
    let ce = root.path().join("base-ce");
    let de = root.path().join("base-de");
    let package = PackageName::new("com.example.app").unwrap();
    let canonical_ce = ce.join(package.as_str());
    let canonical_de = de.join(package.as_str());
    fs::create_dir_all(&canonical_ce).unwrap();
    fs::create_dir_all(&canonical_de).unwrap();
    fs::set_permissions(&canonical_ce, fs::Permissions::from_mode(0o710)).unwrap();
    fs::set_permissions(&canonical_de, fs::Permissions::from_mode(0o750)).unwrap();
    let ce_slots = root.path().join("slots-ce");
    let de_slots = root.path().join("slots-de");
    let mut storage = open_slot_storage(&ce_slots, &de_slots, &ce, &de);
    let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
    let slot = aggregate
        .reserve_slot(DisplayName::new("Blank").unwrap())
        .unwrap();

    storage
        .materialize(&package, &slot, SeedMode::Blank)
        .unwrap();

    let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
    assert_eq!(fs::metadata(target_ce).unwrap().mode() & 0o777, 0o710);
    assert_eq!(fs::metadata(target_de).unwrap().mode() & 0o777, 0o750);
}

#[test]
fn failed_pair_materialization_leaves_no_half_slot() {
    let root = tempfile::tempdir().unwrap();
    let ce = root.path().join("base-ce");
    let de = root.path().join("base-de");
    let package = PackageName::new("com.example.app").unwrap();
    fs::create_dir_all(ce.join(package.as_str())).unwrap();
    let ce_slots = root.path().join("slots-ce");
    let de_slots = root.path().join("slots-de");
    let mut storage = open_slot_storage(&ce_slots, &de_slots, &ce, &de);
    let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
    let slot = aggregate
        .reserve_slot(DisplayName::new("Clone").unwrap())
        .unwrap();

    let result = storage.materialize(&package, &slot, SeedMode::CloneBase);

    assert!(result.is_err());
    let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
    assert!(!target_ce.exists());
    assert!(!target_de.exists());
}

#[test]
fn discard_removes_published_and_stale_temporary_slot_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let slot = SlotId::numbered(1);
    let ce_slots = root.path().join("slots-ce");
    let de_slots = root.path().join("slots-de");
    let ce_package = ce_slots.join(package.as_str());
    let de_package = de_slots.join(package.as_str());
    let published_ce = ce_package.join(slot.as_str());
    let stale_ce = ce_package.join(".slot-1.tmp-111");
    let stale_de = de_package.join(".slot-1.tmp-222");
    let unrelated = de_package.join(".slot-10.tmp-333");
    for directory in [&published_ce, &stale_ce, &stale_de, &unrelated] {
        fs::create_dir_all(directory).unwrap();
    }
    let mut storage = open_slot_storage(
        &ce_slots,
        &de_slots,
        &root.path().join("base-ce"),
        &root.path().join("base-de"),
    );

    storage.discard(&package, &slot).unwrap();

    assert!(!published_ce.exists());
    assert!(!stale_ce.exists());
    assert!(!stale_de.exists());
    assert!(unrelated.exists());
}

#[test]
fn discard_package_removes_every_ce_de_slot_and_temporary_artifact() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let other = PackageName::new("com.example.other").unwrap();
    let ce_slots = root.path().join("slots-ce");
    let de_slots = root.path().join("slots-de");
    for slots_root in [&ce_slots, &de_slots] {
        for path in [
            slots_root.join(package.as_str()).join("slot-1"),
            slots_root.join(package.as_str()).join(".slot-2.tmp-111"),
            slots_root.join(other.as_str()).join("slot-1"),
        ] {
            fs::create_dir_all(path).unwrap();
        }
    }
    let mut storage = open_slot_storage(
        &ce_slots,
        &de_slots,
        &root.path().join("base-ce"),
        &root.path().join("base-de"),
    );

    storage.discard_package(&package).unwrap();

    assert!(!ce_slots.join(package.as_str()).exists());
    assert!(!de_slots.join(package.as_str()).exists());
    assert!(ce_slots.join(other.as_str()).join("slot-1").is_dir());
    assert!(de_slots.join(other.as_str()).join("slot-1").is_dir());
}

#[test]
fn opening_storage_hardens_only_source_ancestors() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let ce_storage = root.path().join("ce/uclone-slices-v2");
    let de_storage = root.path().join("de/uclone-slices-v2");
    let ce_slots = ce_storage.join("slots");
    let de_slots = de_storage.join("slots");
    let ce_package = ce_slots.join(package.as_str());
    let de_package = de_slots.join(package.as_str());
    let ce_slot = ce_package.join("slot-1");
    let de_slot = de_package.join("slot-1");
    for directory in [&ce_slot, &de_slot] {
        fs::create_dir_all(directory).unwrap();
        fs::set_permissions(directory, fs::Permissions::from_mode(0o710)).unwrap();
        fs::write(directory.join("account.db"), b"preserve").unwrap();
        fs::set_permissions(
            directory.join("account.db"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
    }
    for directory in [
        &ce_storage,
        &de_storage,
        &ce_slots,
        &de_slots,
        &ce_package,
        &de_package,
    ] {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o777)).unwrap();
    }

    let _storage = open_slot_storage(
        &ce_slots,
        &de_slots,
        &root.path().join("canonical-ce"),
        &root.path().join("canonical-de"),
    );

    for directory in [
        &ce_storage,
        &de_storage,
        &ce_slots,
        &de_slots,
        &ce_package,
        &de_package,
    ] {
        assert_eq!(fs::metadata(directory).unwrap().mode() & 0o777, 0o700);
    }
    for directory in [&ce_slot, &de_slot] {
        assert_eq!(fs::metadata(directory).unwrap().mode() & 0o777, 0o710);
        assert_eq!(
            fs::metadata(directory.join("account.db")).unwrap().mode() & 0o777,
            0o640,
        );
        assert_eq!(fs::read(directory.join("account.db")).unwrap(), b"preserve");
    }
}

#[test]
fn source_ancestor_symlink_and_non_directory_are_rejected() {
    for kind in ["symlink", "file"] {
        let root = tempfile::tempdir().unwrap();
        let ce_storage = root.path().join("ce/uclone-slices-v2");
        fs::create_dir_all(ce_storage.parent().unwrap()).unwrap();
        if kind == "symlink" {
            let target = root.path().join("elsewhere");
            fs::create_dir(&target).unwrap();
            symlink(target, &ce_storage).unwrap();
        } else {
            fs::write(&ce_storage, b"not a directory").unwrap();
        }
        let ancestor = fs::metadata(root.path()).unwrap();

        let result = FileSlotStorage::open_with_owner(
            ce_storage.join("slots"),
            root.path().join("de/uclone-slices-v2/slots"),
            root.path().join("canonical-ce"),
            root.path().join("canonical-de"),
            DirectoryOwner::new(ancestor.uid(), ancestor.gid()),
        );

        assert!(result.is_err(), "kind: {kind}");
    }
}

#[test]
fn restore_staging_replaces_and_rolls_back_a_ce_de_slot_pair() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let canonical_ce_root = root.path().join("canonical-ce");
    let canonical_de_root = root.path().join("canonical-de");
    fs::create_dir_all(canonical_ce_root.join(package.as_str())).unwrap();
    fs::create_dir_all(canonical_de_root.join(package.as_str())).unwrap();
    fs::create_dir(root.path().join("ce")).unwrap();
    fs::create_dir(root.path().join("de")).unwrap();
    let ce_slots = root.path().join("ce/uclone-slices-v2/slots");
    let de_slots = root.path().join("de/uclone-slices-v2/slots");
    let mut storage =
        open_slot_storage(&ce_slots, &de_slots, &canonical_ce_root, &canonical_de_root);
    let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
    let slot = aggregate
        .reserve_slot(DisplayName::new("Work").unwrap())
        .unwrap();
    storage
        .materialize(&package, &slot, SeedMode::Blank)
        .unwrap();
    let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
    fs::write(target_ce.join("old-ce-a"), b"old-a").unwrap();
    fs::write(target_ce.join("old-ce-b"), b"old-b").unwrap();
    fs::write(target_de.join("old-de-a"), b"old-a").unwrap();
    fs::write(target_de.join("old-de-b"), b"old-b").unwrap();
    let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
    let transfer = TransferId::new("transfer-1").unwrap();
    let account = ArchiveAccountId::new("account-1").unwrap();
    let staging = storage
        .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
        .unwrap();
    fs::write(Path::new(&staging[0].ce_path).join("new-ce"), b"new").unwrap();
    fs::write(Path::new(&staging[0].de_path).join("new-de"), b"new").unwrap();
    storage
        .set_available_bytes(Path::new(&staging[0].ce_path), 0)
        .unwrap();

    let non_base_exdev = fail_next_rename_into(&[&target_ce]);
    let first_attempt = storage.replace_account(
        &package,
        &token,
        &transfer,
        &account,
        slot.id(),
        &RestorePolicy::Replace,
    );
    assert!(
        first_attempt
            .as_ref()
            .is_err_and(|error| error.to_string().contains("Cross-device link"))
    );
    assert!(non_base_exdev.was_triggered());
    drop(non_base_exdev);

    storage
        .replace_account(
            &package,
            &token,
            &transfer,
            &account,
            slot.id(),
            &RestorePolicy::Replace,
        )
        .unwrap();

    assert_eq!(fs::read(target_ce.join("new-ce")).unwrap(), b"new");
    assert_eq!(fs::read(target_de.join("new-de")).unwrap(), b"new");
    assert!(!target_ce.join("old-ce-a").exists());
    assert!(!target_de.join("old-de-a").exists());

    let (rollback_ce, rollback_de) = storage.rollback_paths(&package, &token, slot.id()).unwrap();
    remove_tree_if_exists(&target_ce).unwrap();
    remove_tree_if_exists(&target_de).unwrap();
    fs::write(rollback_ce.join("restore-started"), b"").unwrap();
    fs::write(rollback_de.join("restore-started"), b"").unwrap();
    fs::rename(rollback_de.join("old"), &target_de).unwrap();

    storage
        .rollback_account(&package, &token, slot.id(), &RestorePolicy::Replace)
        .unwrap();

    assert_eq!(fs::read(target_ce.join("old-ce-a")).unwrap(), b"old-a");
    assert_eq!(fs::read(target_ce.join("old-ce-b")).unwrap(), b"old-b");
    assert_eq!(fs::read(target_de.join("old-de-a")).unwrap(), b"old-a");
    assert_eq!(fs::read(target_de.join("old-de-b")).unwrap(), b"old-b");
    assert!(!target_ce.join("new-ce").exists());
    assert!(!target_de.join("new-de").exists());
}

#[test]
fn slot_restore_preserves_resources_while_replacing_and_rolling_back_account_data() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.tencent.tmgp.dfm").unwrap();
    let canonical_ce_root = root.path().join("canonical-ce");
    let canonical_de_root = root.path().join("canonical-de");
    fs::create_dir_all(canonical_ce_root.join(package.as_str())).unwrap();
    fs::create_dir_all(canonical_de_root.join(package.as_str())).unwrap();
    fs::create_dir(root.path().join("ce")).unwrap();
    fs::create_dir(root.path().join("de")).unwrap();
    let mut storage = open_slot_storage(
        &root.path().join("ce/uclone-slices-v2/slots"),
        &root.path().join("de/uclone-slices-v2/slots"),
        &canonical_ce_root,
        &canonical_de_root,
    );
    let mut aggregate = PackageAggregate::enrolled(package.clone(), identity());
    let slot = aggregate
        .reserve_slot(DisplayName::new("Work").unwrap())
        .unwrap();
    storage
        .materialize(&package, &slot, SeedMode::Blank)
        .unwrap();
    let (target_ce, target_de) = storage.slot_paths(&package, slot.id());
    fs::write(target_ce.join("old-account"), b"old-account").unwrap();
    fs::write(target_de.join("old-device"), b"old-device").unwrap();
    let puffer =
        target_ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer/download/resource.pak");
    let paks =
        target_ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Dolphin/1.2.3/Paks/base.pak");
    fs::create_dir_all(puffer.parent().unwrap()).unwrap();
    fs::create_dir_all(paks.parent().unwrap()).unwrap();
    fs::write(&puffer, b"puffer-resource").unwrap();
    fs::write(&paks, b"dolphin-resource").unwrap();
    fs::create_dir_all(target_ce.join("cache")).unwrap();
    fs::write(target_ce.join("cache/transient"), b"discard").unwrap();

    let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
    let transfer = TransferId::new("transfer-preserve-slot").unwrap();
    let account = ArchiveAccountId::new("account-preserve-slot").unwrap();
    let staging = storage
        .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
        .unwrap();
    fs::write(
        Path::new(&staging[0].ce_path).join("new-account"),
        b"new-account",
    )
    .unwrap();
    fs::write(
        Path::new(&staging[0].de_path).join("new-device"),
        b"new-device",
    )
    .unwrap();
    let policy = resource_preserve_policy();

    let (rollback_ce, _) = storage.rollback_paths(&package, &token, slot.id()).unwrap();
    let interrupted_resource =
        rollback_ce.join("preserved/files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer");
    let interrupted_move = fail_next_rename_into(&[&interrupted_resource]);
    let interrupted =
        storage.replace_account(&package, &token, &transfer, &account, slot.id(), &policy);
    assert!(
        interrupted
            .as_ref()
            .is_err_and(|error| error.to_string().contains("Cross-device link"))
    );
    assert!(interrupted_move.was_triggered());
    assert_eq!(
        fs::read(target_ce.join("old-account")).unwrap(),
        b"old-account"
    );
    assert_eq!(fs::read(&puffer).unwrap(), b"puffer-resource");
    assert_eq!(fs::read(&paks).unwrap(), b"dolphin-resource");
    assert!(!rollback_ce.exists());
    drop(interrupted_move);

    storage
        .replace_account(&package, &token, &transfer, &account, slot.id(), &policy)
        .unwrap();

    assert_eq!(
        fs::read(target_ce.join("new-account")).unwrap(),
        b"new-account"
    );
    assert_eq!(
        fs::read(target_de.join("new-device")).unwrap(),
        b"new-device"
    );
    assert_eq!(fs::read(&puffer).unwrap(), b"puffer-resource");
    assert_eq!(fs::read(&paks).unwrap(), b"dolphin-resource");
    assert!(!target_ce.join("old-account").exists());
    assert!(!target_ce.join("cache").exists());

    storage
        .rollback_account(&package, &token, slot.id(), &policy)
        .unwrap();

    assert_eq!(
        fs::read(target_ce.join("old-account")).unwrap(),
        b"old-account"
    );
    assert_eq!(
        fs::read(target_de.join("old-device")).unwrap(),
        b"old-device"
    );
    assert_eq!(fs::read(&puffer).unwrap(), b"puffer-resource");
    assert_eq!(fs::read(&paks).unwrap(), b"dolphin-resource");
    assert!(!target_ce.join("new-account").exists());
}

#[test]
fn base_restore_capacity_excludes_preserved_resource_trees() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.tencent.tmgp.dfm").unwrap();
    let canonical_ce_root = root.path().join("canonical-ce");
    let canonical_de_root = root.path().join("canonical-de");
    let target_ce = canonical_ce_root.join(package.as_str());
    let target_de = canonical_de_root.join(package.as_str());
    fs::create_dir_all(&target_ce).unwrap();
    fs::create_dir_all(&target_de).unwrap();
    fs::write(target_ce.join("old-account"), b"old-account").unwrap();
    fs::write(target_de.join("old-device"), b"old-device").unwrap();
    let resource = target_ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer/resource.pak");
    fs::create_dir_all(resource.parent().unwrap()).unwrap();
    fs::write(&resource, vec![0x5a; 1024 * 1024]).unwrap();
    fs::create_dir(root.path().join("ce")).unwrap();
    fs::create_dir(root.path().join("de")).unwrap();
    let mut storage = open_slot_storage(
        &root.path().join("ce/uclone-slices-v2/slots"),
        &root.path().join("de/uclone-slices-v2/slots"),
        &canonical_ce_root,
        &canonical_de_root,
    );
    let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
    let transfer = TransferId::new("transfer-preserve-base").unwrap();
    let account = ArchiveAccountId::new("account-preserve-base").unwrap();
    let staging = storage
        .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
        .unwrap();
    let staged_ce = Path::new(&staging[0].ce_path);
    let staged_de = Path::new(&staging[0].de_path);
    fs::write(staged_ce.join("new-account"), b"new-account").unwrap();
    fs::write(staged_de.join("new-device"), b"new-device").unwrap();
    let policy = resource_preserve_policy();
    let required_without_resources =
        copied_tree_bytes_excluding(&target_ce, &policy.paths_for(RestoreDomain::Ce)).unwrap()
            + copied_tree_bytes_excluding(&target_de, &policy.paths_for(RestoreDomain::De))
                .unwrap()
            + copied_tree_bytes(staged_ce).unwrap()
            + copied_tree_bytes(staged_de).unwrap();
    assert!(copied_tree_bytes(&target_ce).unwrap() > required_without_resources);
    storage
        .set_available_bytes(staged_ce, required_without_resources)
        .unwrap();

    storage
        .replace_account(
            &package,
            &token,
            &transfer,
            &account,
            &SlotId::base(),
            &policy,
        )
        .unwrap();

    assert_eq!(
        fs::read(target_ce.join("new-account")).unwrap(),
        b"new-account"
    );
    assert_eq!(
        fs::read(target_de.join("new-device")).unwrap(),
        b"new-device"
    );
    assert_eq!(fs::metadata(&resource).unwrap().len(), 1024 * 1024);
    assert!(!target_ce.join("old-account").exists());

    storage
        .rollback_account(&package, &token, &SlotId::base(), &policy)
        .unwrap();

    assert_eq!(
        fs::read(target_ce.join("old-account")).unwrap(),
        b"old-account"
    );
    assert_eq!(
        fs::read(target_de.join("old-device")).unwrap(),
        b"old-device"
    );
    assert_eq!(fs::metadata(&resource).unwrap().len(), 1024 * 1024);
    assert!(!target_ce.join("new-account").exists());
}

#[test]
fn preserve_policy_rejects_staged_overlap_and_target_symlink_before_mutation() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.tencent.tmgp.dfm").unwrap();
    let canonical_ce_root = root.path().join("canonical-ce");
    let canonical_de_root = root.path().join("canonical-de");
    let target_ce = canonical_ce_root.join(package.as_str());
    let target_de = canonical_de_root.join(package.as_str());
    fs::create_dir_all(&target_ce).unwrap();
    fs::create_dir_all(&target_de).unwrap();
    fs::write(target_ce.join("old-account"), b"old-account").unwrap();
    fs::write(target_de.join("old-device"), b"old-device").unwrap();
    fs::create_dir(root.path().join("ce")).unwrap();
    fs::create_dir(root.path().join("de")).unwrap();
    let mut storage = open_slot_storage(
        &root.path().join("ce/uclone-slices-v2/slots"),
        &root.path().join("de/uclone-slices-v2/slots"),
        &canonical_ce_root,
        &canonical_de_root,
    );
    let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
    let transfer = TransferId::new("transfer-preserve-conflict").unwrap();
    let account = ArchiveAccountId::new("account-preserve-conflict").unwrap();
    let staging = storage
        .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
        .unwrap();
    let staged_resource =
        Path::new(&staging[0].ce_path).join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer");
    fs::create_dir_all(&staged_resource).unwrap();
    let policy = resource_preserve_policy();
    assert!(
        storage
            .validate_staged_account(&package, &token, &transfer, &account, &policy)
            .is_err()
    );
    remove_tree_if_exists(&staged_resource).unwrap();

    let outside = root.path().join("outside-resource");
    fs::create_dir(&outside).unwrap();
    let target_resource = target_ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer");
    fs::create_dir_all(target_resource.parent().unwrap()).unwrap();
    symlink(&outside, &target_resource).unwrap();
    fs::write(
        Path::new(&staging[0].ce_path).join("new-account"),
        b"new-account",
    )
    .unwrap();
    fs::write(
        Path::new(&staging[0].de_path).join("new-device"),
        b"new-device",
    )
    .unwrap();

    assert!(
        storage
            .replace_account(
                &package,
                &token,
                &transfer,
                &account,
                &SlotId::base(),
                &policy,
            )
            .is_err()
    );
    assert_eq!(
        fs::read(target_ce.join("old-account")).unwrap(),
        b"old-account"
    );
    assert_eq!(
        fs::read(target_de.join("old-device")).unwrap(),
        b"old-device"
    );
    assert!(!target_ce.join("new-account").exists());
}

#[test]
fn base_rollback_retries_from_a_complete_old_copy_after_partial_progress() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let canonical_ce_root = root.path().join("canonical-ce");
    let canonical_de_root = root.path().join("canonical-de");
    let target_ce = canonical_ce_root.join(package.as_str());
    let target_de = canonical_de_root.join(package.as_str());
    fs::create_dir_all(&target_ce).unwrap();
    fs::create_dir_all(&target_de).unwrap();
    for name in ["old-ce-a", "old-ce-b"] {
        fs::write(target_ce.join(name), name.as_bytes()).unwrap();
    }
    fs::create_dir(target_ce.join("nested")).unwrap();
    fs::write(target_ce.join("nested/old-ce-c"), b"old-ce-c").unwrap();
    fs::write(target_ce.join("nested/old-ce-d"), b"old-ce-d").unwrap();
    for name in ["old-de-a", "old-de-b"] {
        fs::write(target_de.join(name), name.as_bytes()).unwrap();
    }
    fs::create_dir(root.path().join("ce")).unwrap();
    fs::create_dir(root.path().join("de")).unwrap();
    let mut storage = open_slot_storage(
        &root.path().join("ce/uclone-slices-v2/slots"),
        &root.path().join("de/uclone-slices-v2/slots"),
        &canonical_ce_root,
        &canonical_de_root,
    );
    let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
    let transfer = TransferId::new("transfer-base").unwrap();
    let account = ArchiveAccountId::new("account-base").unwrap();
    let staging = storage
        .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
        .unwrap();
    fs::write(Path::new(&staging[0].ce_path).join("new-ce"), b"new").unwrap();
    fs::write(Path::new(&staging[0].de_path).join("new-de"), b"new").unwrap();
    let base_rename_guard = fail_next_rename_into(&[&target_ce, &target_de]);
    storage
        .replace_account(
            &package,
            &token,
            &transfer,
            &account,
            &SlotId::base(),
            &RestorePolicy::Replace,
        )
        .unwrap();
    assert!(!base_rename_guard.was_triggered());
    assert_eq!(fs::read(target_ce.join("new-ce")).unwrap(), b"new");
    assert_eq!(fs::read(target_de.join("new-de")).unwrap(), b"new");

    let (rollback_ce, rollback_de) = storage
        .rollback_paths(&package, &token, &SlotId::base())
        .unwrap();
    remove_directory_contents(&target_ce).unwrap();
    remove_directory_contents(&target_de).unwrap();
    fs::write(rollback_ce.join("restore-started"), b"").unwrap();
    fs::write(rollback_de.join("restore-started"), b"").unwrap();
    fs::copy(rollback_ce.join("old/old-ce-a"), target_ce.join("old-ce-a")).unwrap();
    fs::create_dir(target_ce.join("nested")).unwrap();
    fs::copy(
        rollback_ce.join("old/nested/old-ce-c"),
        target_ce.join("nested/old-ce-c"),
    )
    .unwrap();
    fs::copy(rollback_de.join("old/old-de-a"), target_de.join("old-de-a")).unwrap();

    storage
        .rollback_account(&package, &token, &SlotId::base(), &RestorePolicy::Replace)
        .unwrap();
    assert!(!base_rename_guard.was_triggered());

    assert_eq!(fs::read(target_ce.join("old-ce-a")).unwrap(), b"old-ce-a");
    assert_eq!(fs::read(target_ce.join("old-ce-b")).unwrap(), b"old-ce-b");
    assert_eq!(
        fs::read(target_ce.join("nested/old-ce-c")).unwrap(),
        b"old-ce-c"
    );
    assert_eq!(
        fs::read(target_ce.join("nested/old-ce-d")).unwrap(),
        b"old-ce-d"
    );
    assert_eq!(fs::read(target_de.join("old-de-a")).unwrap(), b"old-de-a");
    assert_eq!(fs::read(target_de.join("old-de-b")).unwrap(), b"old-de-b");
    assert!(!target_ce.join("new-ce").exists());
    assert!(!target_de.join("new-de").exists());
}

#[test]
fn base_de_copy_failure_rolls_back_the_successful_ce_and_de_pair() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let canonical_ce_root = root.path().join("canonical-ce");
    let canonical_de_root = root.path().join("canonical-de");
    let target_ce = canonical_ce_root.join(package.as_str());
    let target_de = canonical_de_root.join(package.as_str());
    fs::create_dir_all(&target_ce).unwrap();
    fs::create_dir_all(&target_de).unwrap();
    fs::write(target_ce.join("old-ce"), b"old-ce").unwrap();
    fs::write(target_de.join("old-de"), b"old-de").unwrap();
    let resource = target_ce.join("files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer/resource.pak");
    fs::create_dir_all(resource.parent().unwrap()).unwrap();
    fs::write(&resource, b"resource").unwrap();
    fs::create_dir(root.path().join("ce")).unwrap();
    fs::create_dir(root.path().join("de")).unwrap();
    let mut storage = open_slot_storage(
        &root.path().join("ce/uclone-slices-v2/slots"),
        &root.path().join("de/uclone-slices-v2/slots"),
        &canonical_ce_root,
        &canonical_de_root,
    );
    let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
    let transfer = TransferId::new("transfer-base-de-failure").unwrap();
    let account = ArchiveAccountId::new("account-base-de-failure").unwrap();
    let staging = storage
        .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
        .unwrap();
    let staged_ce = Path::new(&staging[0].ce_path);
    let staged_de = Path::new(&staging[0].de_path);
    fs::write(staged_ce.join("new-ce"), b"new-ce").unwrap();
    fs::write(staged_de.join("new-de"), b"new-de").unwrap();

    let _copy_failure = fail_next_copy(staged_de, &target_de);
    let policy = resource_preserve_policy();
    let result = storage.replace_account(
        &package,
        &token,
        &transfer,
        &account,
        &SlotId::base(),
        &policy,
    );

    assert!(result.is_err_and(|error| error.to_string().contains("injected copy failure")));
    assert_eq!(fs::read(target_ce.join("old-ce")).unwrap(), b"old-ce");
    assert_eq!(fs::read(target_de.join("old-de")).unwrap(), b"old-de");
    assert_eq!(fs::read(&resource).unwrap(), b"resource");
    assert!(!target_ce.join("new-ce").exists());
    assert!(!target_de.join("new-de").exists());
    let (rollback_ce, rollback_de) = storage
        .rollback_paths(&package, &token, &SlotId::base())
        .unwrap();
    assert!(!rollback_ce.exists());
    assert!(!rollback_de.exists());
}

#[test]
fn base_restore_rejects_space_that_covers_rollback_but_not_replacement_copy() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let canonical_ce_root = root.path().join("canonical-ce");
    let canonical_de_root = root.path().join("canonical-de");
    let target_ce = canonical_ce_root.join(package.as_str());
    let target_de = canonical_de_root.join(package.as_str());
    fs::create_dir_all(&target_ce).unwrap();
    fs::create_dir_all(&target_de).unwrap();
    fs::write(target_ce.join("old-ce"), b"old-ce").unwrap();
    fs::write(target_de.join("old-de"), b"old-de").unwrap();
    fs::create_dir(root.path().join("ce")).unwrap();
    fs::create_dir(root.path().join("de")).unwrap();
    let mut storage = open_slot_storage(
        &root.path().join("ce/uclone-slices-v2/slots"),
        &root.path().join("de/uclone-slices-v2/slots"),
        &canonical_ce_root,
        &canonical_de_root,
    );
    let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
    let transfer = TransferId::new("transfer-base-capacity").unwrap();
    let account = ArchiveAccountId::new("account-base-capacity").unwrap();
    let staging = storage
        .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
        .unwrap();
    let staged_ce = Path::new(&staging[0].ce_path);
    let staged_de = Path::new(&staging[0].de_path);
    fs::write(staged_ce.join("new-ce"), b"new-ce").unwrap();
    fs::write(staged_de.join("new-de"), b"new-de").unwrap();
    let ce_required = copied_tree_bytes(&target_ce).unwrap();
    let de_required = copied_tree_bytes(&target_de).unwrap();
    let staged_ce_required = copied_tree_bytes(staged_ce).unwrap();
    let staged_de_required = copied_tree_bytes(staged_de).unwrap();
    assert_eq!(
        fs::metadata(staged_ce).unwrap().dev(),
        fs::metadata(staged_de).unwrap().dev()
    );
    let rollback_only = ce_required + de_required;
    let full_copy_requirement = rollback_only + staged_ce_required + staged_de_required;
    assert!(rollback_only < full_copy_requirement);
    storage
        .set_available_bytes(staged_ce, rollback_only)
        .unwrap();

    let result = storage.replace_account(
        &package,
        &token,
        &transfer,
        &account,
        &SlotId::base(),
        &RestorePolicy::Replace,
    );

    assert!(
        result
            .as_ref()
            .is_err_and(AdapterError::is_insufficient_storage)
    );
    assert_eq!(fs::read(target_ce.join("old-ce")).unwrap(), b"old-ce");
    assert_eq!(fs::read(target_de.join("old-de")).unwrap(), b"old-de");
    assert_eq!(fs::read(staged_ce.join("new-ce")).unwrap(), b"new-ce");
    assert_eq!(fs::read(staged_de.join("new-de")).unwrap(), b"new-de");
    let (rollback_ce, rollback_de) = storage
        .rollback_paths(&package, &token, &SlotId::base())
        .unwrap();
    assert!(!rollback_ce.exists());
    assert!(!rollback_de.exists());
}

#[test]
fn capacity_requirements_are_combined_only_for_the_same_filesystem() {
    assert!(
        ensure_filesystem_capacity([(11, 6, 10), (11, 6, 10)])
            .is_err_and(|error| error.is_insufficient_storage())
    );
    assert!(ensure_filesystem_capacity([(11, 6, 10), (22, 6, 10)]).is_ok());
}

#[test]
fn restore_staging_rejects_an_escaping_symlink_before_replacement() {
    let root = tempfile::tempdir().unwrap();
    let package = PackageName::new("com.example.app").unwrap();
    let canonical_ce_root = root.path().join("canonical-ce");
    let canonical_de_root = root.path().join("canonical-de");
    fs::create_dir_all(canonical_ce_root.join(package.as_str())).unwrap();
    fs::create_dir_all(canonical_de_root.join(package.as_str())).unwrap();
    fs::create_dir(root.path().join("ce")).unwrap();
    fs::create_dir(root.path().join("de")).unwrap();
    let mut storage = open_slot_storage(
        &root.path().join("ce/uclone-slices-v2/slots"),
        &root.path().join("de/uclone-slices-v2/slots"),
        &canonical_ce_root,
        &canonical_de_root,
    );
    let token = AccountIoToken::new("0123456789abcdef0123456789abcdef").unwrap();
    let transfer = TransferId::new("transfer-1").unwrap();
    let account = ArchiveAccountId::new("account-1").unwrap();
    let staging = storage
        .prepare_restore_staging(&package, &token, &transfer, std::slice::from_ref(&account))
        .unwrap();
    symlink(
        "../../../../outside",
        Path::new(&staging[0].ce_path).join("escape"),
    )
    .unwrap();

    assert!(
        storage
            .validate_staged_account(
                &package,
                &token,
                &transfer,
                &account,
                &RestorePolicy::Replace,
            )
            .is_err()
    );
}

#[test]
fn private_directory_verification_rejects_wrong_owner_and_mode() {
    let root = tempfile::tempdir().unwrap();
    let metadata = fs::metadata(root.path()).unwrap();
    let actual_owner = DirectoryOwner::new(metadata.uid(), metadata.gid());
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o777)).unwrap();
    let wrong_mode = fs::metadata(root.path()).unwrap();

    assert!(verify_private_directory(&wrong_mode, actual_owner).is_err());
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let secure = fs::metadata(root.path()).unwrap();
    let wrong_owner = DirectoryOwner::new(secure.uid().saturating_add(1), secure.gid());
    assert!(verify_private_directory(&secure, wrong_owner).is_err());
}

#[test]
fn deep_app_trees_are_measured_and_copied_without_recursion() {
    // A recursive walk of this depth needs far more than the small stack below, while an
    // iterative walk needs only one directory level of it.
    const DEPTH: usize = 1500;
    const SMALL_STACK_BYTES: usize = 256 * 1024;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("s");
    let target = root.path().join("t");
    let mut relative = PathBuf::new();
    for _ in 0..DEPTH {
        relative.push("d");
    }
    let preserved = relative.join("keep");
    fs::create_dir_all(source.join(&preserved)).unwrap();
    fs::write(source.join(&relative).join("leaf"), b"leaf").unwrap();
    fs::create_dir(&target).unwrap();
    let worker_source = source.clone();
    let worker_target = target.clone();

    let (whole, filtered) = std::thread::Builder::new()
        .stack_size(SMALL_STACK_BYTES)
        .spawn(move || {
            let whole = copied_tree_bytes(&worker_source)?;
            let filtered =
                copied_tree_bytes_filtered(&worker_source, &BTreeSet::from([preserved.clone()]))?;
            copy_tree_excluding(&worker_source, &worker_target, &[preserved])?;
            Ok::<_, AdapterError>((whole, filtered))
        })
        .unwrap()
        .join()
        .unwrap()
        .unwrap();

    assert!(filtered < whole);
    assert_eq!(
        fs::read(target.join(&relative).join("leaf")).unwrap(),
        b"leaf"
    );
    assert!(!target.join(&relative).join("keep").exists());
}
