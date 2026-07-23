use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::orphan_gate::probe;
use super::slot_lifecycle_support::ready_package;
use crate::android::{
    CanonicalView, MountCounts, MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
use crate::domain::{
    CommitNonce, DataInodes, GateSnapshot, PackageCandidate, PackageEnabledState, PackageName,
    PackageObservation, SlotId, SlotView, TransactionId, UserId,
};
use crate::journal::{JournalEvent, TransactionSpec};
use crate::registry::PackageRevision;
use crate::service::PackageState;
use crate::slot_metadata::{SlotDisplayName, SlotRecordState, SlotSeedMode};

#[test]
fn active_extension_requires_existing_ready_metadata() {
    for state in [
        None,
        Some(SlotRecordState::Creating),
        Some(SlotRecordState::Quarantined),
        Some(SlotRecordState::Deleted),
    ] {
        let (_root, stores, key, target) = active_extension_fixture(state);
        let mut package_probe = ActiveViewProbe::new(target.inodes());

        let loaded = super::super::state::load(&stores, &mut package_probe, &key).unwrap();

        assert!(
            matches!(loaded, PackageState::RecoveryRequired),
            "unsafe active metadata state was accepted: {state:?}",
        );
    }
}

#[test]
fn active_extension_with_ready_metadata_remains_usable() {
    let (_root, stores, key, target) = active_extension_fixture(Some(SlotRecordState::Ready));
    let mut package_probe = ActiveViewProbe::new(target.inodes());

    let loaded = super::super::state::load(&stores, &mut package_probe, &key).unwrap();

    assert!(matches!(
        loaded,
        PackageState::Ready(snapshot)
            if snapshot.managed().active_slot() == target.slot_id()
                && snapshot.managed().active_inodes() == target.inodes()
    ));
}

fn active_extension_fixture(
    metadata_state: Option<SlotRecordState>,
) -> (
    tempfile::TempDir,
    super::super::stores::ProductionStores,
    crate::domain::PackageKey,
    SlotView,
) {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let (stores, key, managed) = ready_package(root.path());
    let target = SlotView::new(
        SlotId::parse("slot-active-ready-proof").unwrap(),
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
    publish_metadata(&stores, &key, &managed, &target, metadata_state);
    commit_target(&stores, &managed, &target);
    (root, stores, key, target)
}

fn publish_metadata(
    stores: &super::super::stores::ProductionStores,
    key: &crate::domain::PackageKey,
    managed: &crate::domain::ManagedPackage,
    target: &SlotView,
    state: Option<SlotRecordState>,
) {
    let Some(state) = state else { return };
    let name = SlotDisplayName::parse("Active extension").unwrap();
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

fn commit_target(
    stores: &super::super::stores::ProductionStores,
    managed: &crate::domain::ManagedPackage,
    target: &SlotView,
) {
    let spec = TransactionSpec::new(
        TransactionId::parse("tx-active-ready-proof").unwrap(),
        managed.clone(),
        target.clone(),
        GateSnapshot::new(PackageEnabledState::Default, false),
        "boot-active-ready-proof",
    )
    .unwrap();
    let nonce = CommitNonce::parse("nonce-active-ready-proof").unwrap();
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
    stores
        .registry
        .append(&PackageRevision::committed(&spec, managed.base_inodes(), nonce.clone()).unwrap())
        .unwrap();
    for event in [
        JournalEvent::RegistryCommitted { nonce },
        JournalEvent::GateReleased,
        JournalEvent::Completed,
    ] {
        stores.journal.append(spec.transaction_id(), event).unwrap();
    }
}

#[derive(Debug)]
struct ActiveViewProbe {
    inodes: DataInodes,
}

impl ActiveViewProbe {
    const fn new(inodes: DataInodes) -> Self {
        Self { inodes }
    }
}

impl PackageProbe for ActiveViewProbe {
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Ok(MountNamespaceProof::new(7, 7))
    }

    fn observe_package(
        &mut self,
        _: &PackageName,
        _: UserId,
    ) -> Result<PackageObservation, ProbeError> {
        Ok(PackageObservation::new(
            probe::identity(),
            probe::base_inodes(),
            self.inodes,
            self.inodes,
            false,
        ))
    }

    fn inspect_package(
        &mut self,
        _: &PackageName,
        _: UserId,
    ) -> Result<PackageCandidate, ProbeError> {
        Ok(PackageCandidate::new(
            probe::identity(),
            probe::base_inodes(),
            false,
            crate::domain::PackageCompatibility::compatible(),
        ))
    }

    fn gate_snapshot(&mut self, _: &PackageName, _: UserId) -> Result<GateSnapshot, ProbeError> {
        Ok(GateSnapshot::new(PackageEnabledState::Default, false))
    }

    fn running_process_count(&mut self, _: &PackageName, _: UserId) -> Result<u32, ProbeError> {
        Ok(0)
    }

    fn view_proof(&mut self, _: &PackageName, _: UserId) -> Result<ViewProof, ProbeError> {
        Ok(ViewProof::new(
            CanonicalView::new(self.inodes, MountCounts::new(1, 1)),
            self.inodes,
            self.inodes,
        ))
    }

    fn slot_inodes(
        &mut self,
        _: &PackageName,
        _: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        Ok(Some(self.inodes))
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(true)
    }
}
