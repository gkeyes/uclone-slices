#![doc = "Durable rescue gate-lease failure coverage for the Android backend."]
#![allow(clippy::unwrap_used, reason = "validated test fixtures")]

use uclone_slot_runtime::android::{
    AndroidBackend, AndroidCommand, CommandError, CommandKind, CommandRunner, EmergencyGateLease,
    GateLease, GateLeaseError, GateLeaseStore, MountNamespaceProof, PackageProbe, ProbeError,
    StoredGateLease, ViewProof,
};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, PackageObservation, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::reconcile::RecoveryBackend;
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};

#[derive(Debug, Default)]
struct RecordingRunner(Vec<AndroidCommand>);

impl CommandRunner for RecordingRunner {
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError> {
        self.0.push(command.clone());
        Ok(())
    }
}

#[derive(Debug)]
struct GateProbe(GateSnapshot);

impl PackageProbe for GateProbe {
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Ok(MountNamespaceProof::new(7, 7))
    }

    fn observe_package(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<PackageObservation, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn gate_snapshot(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<GateSnapshot, ProbeError> {
        Ok(self.0)
    }

    fn running_process_count(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<u32, ProbeError> {
        Ok(0)
    }

    fn view_proof(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<ViewProof, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        _slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(true)
    }
}

#[derive(Debug)]
struct RejectingLeaseStore;

impl GateLeaseStore for RejectingLeaseStore {
    fn artifact_exists(&mut self, _package: &PackageName) -> Result<bool, GateLeaseError> {
        Ok(false)
    }

    fn load(&mut self, _package: &PackageName) -> Result<Option<StoredGateLease>, GateLeaseError> {
        Ok(None)
    }

    fn persist_emergency(&mut self, _lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }
    fn mark_emergency_held(&mut self, _lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }
    fn persist_enrolled(&mut self, _lease: &GateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }

    fn confirm_enrollment(&mut self, _lease: &GateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }

    fn retire(&mut self, _expected: &GateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }
}

#[derive(Debug)]
struct CorruptLeaseStore;

impl GateLeaseStore for CorruptLeaseStore {
    fn artifact_exists(&mut self, _package: &PackageName) -> Result<bool, GateLeaseError> {
        Ok(true)
    }

    fn load(&mut self, _package: &PackageName) -> Result<Option<StoredGateLease>, GateLeaseError> {
        Err(GateLeaseError::InvalidArtifact)
    }

    fn persist_emergency(&mut self, _lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }
    fn mark_emergency_held(&mut self, _lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }

    fn persist_enrolled(&mut self, _lease: &GateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }

    fn confirm_enrollment(&mut self, _lease: &GateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }

    fn retire(&mut self, _expected: &GateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }
}

fn managed() -> ManagedPackage {
    let base = DataInodes::new(101, 202).unwrap();
    ManagedPackage::new(
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
        AppIdentity::new(
            10_321,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            7,
            "/data/app/slotprobe/base.apk",
        )
        .unwrap(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap()
}

#[test]
fn capture_rejects_an_unpersisted_lease_before_disable_command() {
    let package = managed();
    let snapshot = GateSnapshot::new(PackageEnabledState::Default, false);
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        GateProbe(snapshot),
        RejectingLeaseStore,
    );

    let error = backend.capture_gate_snapshot(&package).unwrap_err();

    assert_eq!(
        error,
        PlatformError::CaptureGateSnapshot {
            detail: "gate_lease_persist_failed".to_owned(),
        }
    );
    assert!(backend.runner().0.is_empty());
}

#[test]
fn retirement_requires_a_confirmed_enrolled_lease_without_platform_mutation() {
    let package = managed();
    let snapshot = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        GateProbe(snapshot),
        RejectingLeaseStore,
    );

    let error = backend.retire_gate_lease(&package).unwrap_err();

    assert_eq!(
        error,
        PlatformError::RetireGateLease {
            detail: "enrolled_gate_lease_required".to_owned(),
        }
    );
    assert!(backend.runner().0.is_empty());
}

#[test]
fn missing_orphan_lease_causes_no_platform_mutation() {
    let package = managed();
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        GateProbe(GateSnapshot::new(PackageEnabledState::Default, false)),
        RejectingLeaseStore,
    );

    let snapshot = backend
        .emergency_gate_if_leased(package.package_name())
        .unwrap();

    assert_eq!(snapshot, None);
    assert!(backend.runner().0.is_empty());
}

#[test]
fn corrupt_orphan_lease_is_contained_before_the_typed_error() {
    let package = managed();
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        GateProbe(held),
        CorruptLeaseStore,
    );

    let error = backend
        .emergency_gate_if_leased(package.package_name())
        .unwrap_err();

    assert_eq!(
        error,
        PlatformError::CaptureGateSnapshot {
            detail: "gate_lease_load_failed".to_owned(),
        }
    );
    let kinds: Vec<_> = backend
        .runner()
        .0
        .iter()
        .map(AndroidCommand::kind)
        .collect();
    assert_eq!(kinds, [CommandKind::DisableUser, CommandKind::ForceStop]);
}
