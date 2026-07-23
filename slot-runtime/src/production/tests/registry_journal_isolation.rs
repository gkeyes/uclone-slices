use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::orphan_gate::{probe, stores};
use crate::domain::{
    AppIdentity, CommitNonce, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState,
    PackageKey, PackageName, PackageSupportLevel, SlotId, SlotView, TransactionId, UserId,
};
use crate::journal::{JournalEvent, TransactionSpec};
use crate::lifecycle::LifecycleState;
use crate::protocol::ALLOWED_PACKAGE;
use crate::service::PackageState;

#[test]
fn another_packages_completed_target_does_not_require_registry_for_base() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let stores = stores(root.path());
    let key = allowed_key();
    enroll_ready_base(&stores, &key);
    let other = PackageKey::new(
        PackageName::parse("com.example.other").unwrap(),
        UserId::PRIMARY,
    );
    append_completed_target(&stores, other, "tx-other-completed");
    let mut package_probe = probe::NativeBaseProbe::new();

    let state = super::super::state::load(&stores, &mut package_probe, &key).unwrap();

    assert!(matches!(state, PackageState::Ready(_)));
}

#[test]
fn own_completed_target_without_registry_requires_recovery() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let stores = stores(root.path());
    let key = allowed_key();
    enroll_ready_base(&stores, &key);
    append_completed_target(&stores, key.clone(), "tx-own-completed");
    let mut package_probe = probe::NativeBaseProbe::new();

    let state = super::super::state::load(&stores, &mut package_probe, &key).unwrap();

    assert!(matches!(state, PackageState::RecoveryRequired));
}

fn allowed_key() -> PackageKey {
    PackageKey::new(
        PackageName::parse(ALLOWED_PACKAGE).unwrap(),
        UserId::PRIMARY,
    )
}

fn enroll_ready_base(stores: &super::super::stores::ProductionStores, key: &PackageKey) {
    let base = probe::base_inodes();
    let managed = ManagedPackage::new(
        key.clone(),
        probe::identity(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap();
    stores.enrollment.create(&managed).unwrap();
    stores
        .compatibility_policy
        .create(
            key.package_name(),
            managed.identity(),
            PackageSupportLevel::Supported,
            false,
        )
        .unwrap();
    stores
        .catalog
        .create_base(
            key.clone(),
            base,
            managed.identity().clone(),
            probe::security_profile(),
        )
        .unwrap();
    stores.package_state.initialize(key).unwrap();
}

fn append_completed_target(
    stores: &super::super::stores::ProductionStores,
    key: PackageKey,
    transaction_id: &str,
) {
    let base = DataInodes::new(1_100_001, 1_100_002).unwrap();
    let target = DataInodes::new(1_200_001, 1_200_002).unwrap();
    let managed = ManagedPackage::new(
        key,
        AppIdentity::new(
            10_322,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            1,
            "/data/app/registry-journal-test/base.apk",
        )
        .unwrap(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap();
    let spec = TransactionSpec::new(
        TransactionId::parse(transaction_id).unwrap(),
        managed,
        SlotView::new(SlotId::parse("target").unwrap(), target),
        GateSnapshot::new(PackageEnabledState::Default, false),
        "boot-registry-journal",
    )
    .unwrap();
    let nonce = CommitNonce::parse("nonce-registry-journal").unwrap();
    stores.journal.create(&spec).unwrap();
    for event in [
        JournalEvent::GateHeld,
        JournalEvent::ProcessesQuiesced,
        JournalEvent::Applying,
        JournalEvent::ViewVerified,
        JournalEvent::Committing {
            nonce: nonce.clone(),
        },
        JournalEvent::RegistryCommitted { nonce },
        JournalEvent::GateReleased,
        JournalEvent::Completed,
    ] {
        stores.journal.append(spec.transaction_id(), event).unwrap();
    }
}
