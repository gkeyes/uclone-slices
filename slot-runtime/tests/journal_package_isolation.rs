#![doc = "Cross-package journal corruption isolation tests."]
#![allow(
    clippy::unwrap_used,
    reason = "isolated journal fixtures fail fast on invalid test construction"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, SlotId, SlotView, TransactionId, UserId,
};
use uclone_slot_runtime::journal::{JournalEvent, JournalStore, TransactionSpec};
use uclone_slot_runtime::lifecycle::LifecycleState;

#[test]
fn attributable_corruption_is_visible_only_to_its_package() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let package_a = PackageName::parse("com.example.broken").unwrap();
    let package_b = PackageName::parse("com.example.healthy").unwrap();
    let spec_a = spec("tx-00000041", package_a.clone(), 10_341, 410, 420);
    let spec_b = spec("tx-00000042", package_b.clone(), 10_342, 510, 520);
    store.create(&spec_a).unwrap();
    store
        .append(spec_a.transaction_id(), JournalEvent::GateHeld)
        .unwrap();
    store.create(&spec_b).unwrap();
    fs::write(store.step_path(spec_a.transaction_id(), 2).unwrap(), b"{}").unwrap();

    let key_a = PackageKey::new(package_a.clone(), UserId::PRIMARY);
    let key_b = PackageKey::new(package_b.clone(), UserId::PRIMARY);
    let healthy = store.list_for_package(&key_b).unwrap();
    assert_eq!(healthy.len(), 1);
    assert_eq!(
        healthy.first().map(|journal| journal.spec().package_name()),
        Some(&package_b)
    );
    assert!(store.list_for_package(&key_a).is_err());

    let scan = store.package_names().unwrap();
    assert!(!scan.unattributed_corruption());
    assert!(scan.package_names().contains(&package_a));
    assert!(scan.package_names().contains(&package_b));

    fs::write(store.step_path(spec_a.transaction_id(), 1).unwrap(), b"{}").unwrap();
    let scan = store.package_names().unwrap();
    assert!(scan.unattributed_corruption());
    assert_eq!(scan.package_names(), &[package_b]);
}

#[test]
fn empty_prepublication_staging_is_ignored() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let transaction_id = TransactionId::parse("tx-00000043").unwrap();
    let published = store.transaction_path(&transaction_id).unwrap();
    let staging = published
        .parent()
        .unwrap()
        .join(format!(".new-{}", transaction_id.as_str()));
    fs::create_dir(&staging).unwrap();
    fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).unwrap();

    let scan = store.package_names().unwrap();

    assert!(scan.package_names().is_empty());
    assert!(!scan.unattributed_corruption());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn prepared_prepublication_staging_is_ignored() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let package = PackageName::parse("com.example.staged").unwrap();
    let prepared = spec("tx-00000044", package.clone(), 10_344, 610, 620);
    store.create(&prepared).unwrap();
    let published = store.transaction_path(prepared.transaction_id()).unwrap();
    let staging = published
        .parent()
        .unwrap()
        .join(format!(".new-{}", prepared.transaction_id().as_str()));
    fs::rename(&published, &staging).unwrap();

    let scan = store.package_names().unwrap();

    assert!(scan.package_names().is_empty());
    assert!(!scan.unattributed_corruption());
    assert!(store.list().unwrap().is_empty());
    assert!(
        store
            .list_for_package(&PackageKey::new(package, UserId::PRIMARY))
            .unwrap()
            .is_empty()
    );
}

fn spec(transaction: &str, package: PackageName, uid: u32, ce: u64, de: u64) -> TransactionSpec {
    let base = DataInodes::new(ce, de).unwrap();
    let identity = AppIdentity::new(
        uid,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        1,
        &format!("/data/app/{package}/base.apk"),
    )
    .unwrap();
    let managed = ManagedPackage::new(
        PackageKey::new(package, UserId::PRIMARY),
        identity,
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap();
    TransactionSpec::new(
        TransactionId::parse(transaction).unwrap(),
        managed,
        SlotView::new(
            SlotId::parse("preview").unwrap(),
            DataInodes::new(ce + 1, de + 1).unwrap(),
        ),
        GateSnapshot::new(PackageEnabledState::Default, false),
        "boot-multi-app-isolation",
    )
    .unwrap()
}
