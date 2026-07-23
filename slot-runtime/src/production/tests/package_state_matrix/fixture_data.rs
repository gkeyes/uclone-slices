use super::super::orphan_gate::{probe, stores as fixture_stores};
use super::super::slot_lifecycle_support::ready_package;
use super::fixture::{Expected, Fixture};
use super::matrix_probe::MatrixProbe;
use crate::domain::{
    CommitNonce, DataInodes, GateSnapshot, PackageKey, PackageName, PackageSupportLevel, SlotId,
    SlotView, TransactionId, UserId,
};
use crate::journal::{JournalEvent, TransactionSpec};
use crate::lifecycle::LifecycleState;
use crate::protocol::ALLOWED_PACKAGE;
use crate::registry::PackageRevision;
use crate::slot_metadata::{SlotDisplayName, SlotRecordState, SlotSeedMode};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use tempfile::{TempDir, tempdir};

pub(super) fn root() -> TempDir {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    root
}

pub(super) fn key() -> PackageKey {
    PackageKey::new(
        PackageName::parse(ALLOWED_PACKAGE).unwrap(),
        UserId::PRIMARY,
    )
}

pub(super) fn default_gate() -> GateSnapshot {
    GateSnapshot::new(crate::domain::PackageEnabledState::Default, false)
}

type Stores = super::super::super::stores::ProductionStores;
type Managed = crate::domain::ManagedPackage;

pub(super) fn base_parts() -> (TempDir, Stores, PackageKey, Managed) {
    let root = root();
    let (stores, key, managed) = ready_package(root.path());
    (root, stores, key, managed)
}

pub(super) fn base_case<F>(expected: Expected, probe: MatrixProbe, change: F) -> Fixture
where
    F: FnOnce(&Stores, &PackageKey, &Managed),
{
    let (root, stores, key, managed) = base_parts();
    change(&stores, &key, &managed);
    finish(root, stores, key, probe, expected)
}

pub(super) fn finish(
    root: TempDir,
    stores: Stores,
    key: PackageKey,
    probe: MatrixProbe,
    expected: Expected,
) -> Fixture {
    Fixture {
        _root: root,
        stores,
        key,
        probe,
        expected,
    }
}

pub(super) fn absent() -> Fixture {
    let root = root();
    let stores = fixture_stores(root.path());
    finish(
        root,
        stores,
        key(),
        MatrixProbe::healthy(default_gate()),
        Expected::Absent,
    )
}

pub(super) fn enrolled_case(catalog: bool, state: bool) -> Fixture {
    let (root, stores, key, _) = enrolled(root(), catalog, state);
    finish(
        root,
        stores,
        key,
        MatrixProbe::healthy(default_gate()),
        Expected::Recovery,
    )
}

pub(super) fn enrolled(
    root: TempDir,
    catalog: bool,
    state: bool,
) -> (TempDir, Stores, PackageKey, Managed) {
    let stores = fixture_stores(root.path());
    let key = key();
    let base = probe::base_inodes();
    let managed = Managed::new(
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
    if catalog {
        stores
            .catalog
            .create_base(
                key.clone(),
                base,
                managed.identity().clone(),
                probe::security_profile(),
            )
            .unwrap();
    }
    if state {
        stores.package_state.initialize(&key).unwrap();
    }
    (root, stores, key, managed)
}

pub(super) fn extension(metadata: Option<SlotRecordState>, registry: bool) -> Fixture {
    let (root, stores, key, managed) = base_parts();
    let target = SlotView::new(
        SlotId::parse("slot-state-matrix").unwrap(),
        DataInodes::new(1_410_001, 1_410_002).unwrap(),
    );
    stores
        .catalog
        .append_slot(
            &key,
            target.slot_id().clone(),
            target.inodes(),
            managed.identity(),
            managed.identity().version_code(),
            probe::security_profile(),
        )
        .unwrap();
    if let Some(state) = metadata {
        let name = SlotDisplayName::parse("State matrix extension").unwrap();
        stores
            .slot_metadata
            .create(
                key.package_name(),
                target.slot_id().clone(),
                name.clone(),
                SlotSeedMode::Blank,
                managed.identity().version_code(),
            )
            .unwrap();
        if state != SlotRecordState::Creating {
            stores
                .slot_metadata
                .update(
                    key.package_name(),
                    target.slot_id(),
                    name,
                    state,
                    managed.identity().version_code(),
                )
                .unwrap();
        }
    }
    let spec = TransactionSpec::new(
        TransactionId::parse("tx-state-matrix").unwrap(),
        managed,
        target.clone(),
        default_gate(),
        "boot-state-matrix",
    )
    .unwrap();
    let nonce = CommitNonce::parse("nonce-state-matrix").unwrap();
    stores.journal.create(&spec).unwrap();
    for event in [
        JournalEvent::GateHeld,
        JournalEvent::ProcessesQuiesced,
        JournalEvent::Applying,
        JournalEvent::ViewVerified,
        JournalEvent::Committing {
            nonce: nonce.clone(),
        },
    ] {
        stores.journal.append(spec.transaction_id(), event).unwrap();
    }
    if registry {
        stores
            .registry
            .append(&PackageRevision::committed(&spec, spec.base_inodes(), nonce.clone()).unwrap())
            .unwrap();
    }
    for event in [
        JournalEvent::RegistryCommitted { nonce },
        JournalEvent::GateReleased,
        JournalEvent::Completed,
    ] {
        stores.journal.append(spec.transaction_id(), event).unwrap();
    }
    let expected = if registry && metadata == Some(SlotRecordState::Ready) {
        Expected::ReadyExtension(target.clone())
    } else {
        Expected::Recovery
    };
    finish(
        root,
        stores,
        key,
        MatrixProbe::for_view(target.inodes()),
        expected,
    )
}
