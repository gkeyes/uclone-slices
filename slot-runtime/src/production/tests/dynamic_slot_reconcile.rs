use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::super::composition::ProductionPlatform;
use super::orphan_gate::{probe, runtime::OrphanGateRuntime};
use super::slot_lifecycle_support::{FixedMetadata, TrackingMaterializer, ready_package};
use crate::domain::{GateSnapshot, PackageEnabledState, SlotId};
use crate::slot_metadata::{SlotDisplayName, SlotRecordState, SlotSeedMode};

#[test]
fn reconcile_cleans_exact_dynamic_creating_and_deleted_slot_ids() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let (stores, key, managed) = ready_package(root.path());
    let creating = SlotId::parse("slot-dynamic-creating").unwrap();
    let deleted = SlotId::parse("slot-dynamic-deleted").unwrap();
    create_record(
        &stores,
        &key,
        &managed,
        &creating,
        SlotRecordState::Creating,
    );
    create_record(&stores, &key, &managed, &deleted, SlotRecordState::Deleted);
    let snapshot = GateSnapshot::new(PackageEnabledState::Default, false);
    let runtime = OrphanGateRuntime::enrolled(snapshot, managed);
    let mut platform = ProductionPlatform {
        runtime,
        materializer: TrackingMaterializer::ready_slots(&[creating.clone(), deleted.clone()]),
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: FixedMetadata::new("unused-slot"),
        stores,
        recovery_overrides: BTreeSet::default(),
    };

    let result = platform.cleanup_unpublished_preview(&key);

    assert_eq!(result, Ok(None));
    assert_eq!(
        platform.materializer().cleaned(),
        &[creating.clone(), deleted.clone()],
    );
    assert_eq!(
        platform.materializer().state(&creating),
        crate::materializer::ArtifactState::Absent,
    );
    assert_eq!(
        platform.materializer().state(&deleted),
        crate::materializer::ArtifactState::Absent,
    );
    for slot in [&creating, &deleted] {
        assert_eq!(
            platform
                .stores
                .slot_metadata
                .latest(key.package_name(), slot)
                .unwrap()
                .unwrap()
                .state(),
            SlotRecordState::Deleted,
        );
    }
}

fn create_record(
    stores: &super::super::stores::ProductionStores,
    key: &crate::domain::PackageKey,
    managed: &crate::domain::ManagedPackage,
    slot: &SlotId,
    state: SlotRecordState,
) {
    let name = SlotDisplayName::parse(slot.as_str()).unwrap();
    stores
        .slot_metadata
        .create(
            key.package_name(),
            slot.clone(),
            name.clone(),
            SlotSeedMode::Blank,
            managed.identity().version_code(),
        )
        .unwrap();
    if state == SlotRecordState::Deleted {
        stores
            .slot_metadata
            .update(
                key.package_name(),
                slot,
                name,
                SlotRecordState::Deleted,
                managed.identity().version_code(),
            )
            .unwrap();
    }
}
