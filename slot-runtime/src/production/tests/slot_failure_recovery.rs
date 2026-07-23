use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::super::composition::ProductionPlatform;
use super::orphan_gate::{probe, runtime::OrphanGateRuntime};
use super::slot_lifecycle_support::{FixedMetadata, TrackingMaterializer, ready_package};
use crate::domain::{GateSnapshot, PackageEnabledState, SlotId};
use crate::lifecycle::LifecycleState;
use crate::service::{ServiceError, SwitchExecution};
use crate::slot_metadata::{SlotDisplayName, SlotRecordState, SlotSeedMode};

#[test]
fn create_materialization_failure_contains_package_and_marks_recovery() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let (stores, key, managed) = ready_package(root.path());
    let lifecycle = stores.package_state.clone();
    let snapshot = GateSnapshot::new(PackageEnabledState::Default, false);
    let runtime = OrphanGateRuntime::enrolled(snapshot, managed);
    let mut platform = ProductionPlatform {
        runtime,
        materializer: TrackingMaterializer::publication_failure(),
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: FixedMetadata::new("slot-materialize-fail"),
        stores,
        recovery_overrides: Default::default(),
    };

    let result = platform.do_create_slot(
        &key,
        SlotDisplayName::parse("Materialize failure").unwrap(),
        SlotSeedMode::CloneBase,
    );

    assert_eq!(result, Err(ServiceError::RecoveryRequired));
    assert!(platform.runtime().held);
    let slot = SlotId::parse("slot-materialize-fail").unwrap();
    assert_eq!(
        platform.materializer().cleaned(),
        std::slice::from_ref(&slot),
    );
    assert_eq!(
        platform.materializer().state(&slot),
        crate::materializer::ArtifactState::Absent,
    );
    assert_eq!(
        lifecycle.latest(&key).unwrap().unwrap().lifecycle_state(),
        LifecycleState::RecoveryRequired,
    );
    let record = platform
        .stores
        .slot_metadata
        .latest(key.package_name(), &slot)
        .unwrap()
        .unwrap();
    assert_eq!(record.state(), SlotRecordState::Creating);
}

#[test]
fn create_switch_failure_retains_gate_and_returns_recovery_required() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let (stores, key, managed) = ready_package(root.path());
    let lifecycle = stores.package_state.clone();
    let snapshot = GateSnapshot::new(PackageEnabledState::Default, false);
    let runtime = OrphanGateRuntime::enrolled(snapshot, managed);
    let mut platform = ProductionPlatform {
        runtime,
        materializer: TrackingMaterializer::healthy(),
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: FixedMetadata::new("slot-switch-fail"),
        stores,
        recovery_overrides: Default::default(),
    };

    let result = platform.do_create_slot(
        &key,
        SlotDisplayName::parse("Switch failure").unwrap(),
        SlotSeedMode::Blank,
    );

    assert_eq!(result, Ok(SwitchExecution::RecoveryRequired));
    assert!(platform.runtime().held);
    assert_eq!(
        lifecycle.latest(&key).unwrap().unwrap().lifecycle_state(),
        LifecycleState::RecoveryRequired,
    );
    let record = platform
        .stores
        .slot_metadata
        .latest(
            key.package_name(),
            &SlotId::parse("slot-switch-fail").unwrap(),
        )
        .unwrap()
        .unwrap();
    assert_eq!(record.state(), SlotRecordState::Ready);
}

#[test]
fn delete_tombstones_before_cleanup_and_never_reexposes_ready_slot() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let (stores, key, managed) = ready_package(root.path());
    let slot = SlotId::parse("slot-delete-fail").unwrap();
    stores
        .catalog
        .append_slot(
            &key,
            slot.clone(),
            crate::domain::DataInodes::new(1_300_001, 1_300_002).unwrap(),
            managed.identity(),
            managed.identity().version_code(),
            probe::security_profile(),
        )
        .unwrap();
    let name = SlotDisplayName::parse("Delete failure").unwrap();
    stores
        .slot_metadata
        .create(
            key.package_name(),
            slot.clone(),
            name.clone(),
            SlotSeedMode::CloneBase,
            managed.identity().version_code(),
        )
        .unwrap();
    stores
        .slot_metadata
        .update(
            key.package_name(),
            &slot,
            name,
            SlotRecordState::Ready,
            managed.identity().version_code(),
        )
        .unwrap();
    let snapshot = GateSnapshot::new(PackageEnabledState::Default, false);
    let runtime = OrphanGateRuntime::enrolled(snapshot, managed);
    let mut platform = ProductionPlatform {
        runtime,
        materializer: TrackingMaterializer::cleanup_failure(slot.clone()),
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: FixedMetadata::new("unused-slot"),
        stores,
        recovery_overrides: Default::default(),
    };

    let result = platform.do_delete_slot(&key, &slot);

    assert_eq!(result, Err(ServiceError::RecoveryRequired));
    assert_eq!(
        platform.materializer().cleaned(),
        std::slice::from_ref(&slot),
    );
    assert_eq!(
        platform
            .stores
            .slot_metadata
            .latest(key.package_name(), &slot)
            .unwrap()
            .unwrap()
            .state(),
        SlotRecordState::Deleted,
    );
    assert_eq!(
        platform.do_list_slots(&key),
        Err(ServiceError::RecoveryRequired),
    );
    assert!(platform.runtime().held);
}
