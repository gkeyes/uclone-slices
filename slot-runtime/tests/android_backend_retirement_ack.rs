#![doc = "Gate-lease retirement acknowledgement-loss containment regression."]
#![allow(clippy::unwrap_used, reason = "validated retirement test fixture")]

use std::collections::VecDeque;

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
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};

#[derive(Debug, Default)]
struct RecordingRunner(Vec<CommandKind>);

impl CommandRunner for RecordingRunner {
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError> {
        self.0.push(command.kind());
        Ok(())
    }
}

#[derive(Debug)]
struct AckLostStore(Option<StoredGateLease>);

impl GateLeaseStore for AckLostStore {
    fn artifact_exists(&mut self, _package: &PackageName) -> Result<bool, GateLeaseError> {
        Ok(self.0.is_some())
    }

    fn load(&mut self, _package: &PackageName) -> Result<Option<StoredGateLease>, GateLeaseError> {
        Ok(self.0.clone())
    }

    fn persist_emergency(&mut self, _lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }

    fn mark_emergency_held(&mut self, _lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }

    fn persist_enrolled(&mut self, lease: &GateLease) -> Result<(), GateLeaseError> {
        self.0 = Some(StoredGateLease::Enrolled(lease.clone()));
        Ok(())
    }

    fn confirm_enrollment(&mut self, _lease: &GateLease) -> Result<(), GateLeaseError> {
        Err(GateLeaseError::Unavailable)
    }

    fn retire(&mut self, _expected: &GateLease) -> Result<(), GateLeaseError> {
        self.0 = None;
        Err(GateLeaseError::Unavailable)
    }
}

#[derive(Debug)]
struct GateProbe(VecDeque<GateSnapshot>);

impl PackageProbe for GateProbe {
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Err(ProbeError::Unavailable)
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
        self.0.pop_front().ok_or(ProbeError::InvalidResponse)
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
        Err(ProbeError::Unavailable)
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
fn final_sync_ack_loss_recontains_without_reloading_retired_lease() {
    let package = managed();
    let original = GateSnapshot::new(PackageEnabledState::Enabled, true);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, true);
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        GateProbe(VecDeque::from([original, original, held])),
        AckLostStore(None),
    );
    assert_eq!(backend.capture_gate_snapshot(&package).unwrap(), original);

    let error = backend.retire_gate_lease(&package).unwrap_err();

    assert_eq!(
        error,
        PlatformError::RetireGateLease {
            detail: "gate_lease_remove_failed".to_owned(),
        }
    );
    assert_eq!(
        backend.runner().0,
        [CommandKind::DisableUser, CommandKind::ForceStop]
    );
    assert_eq!(backend.gate_lease_store().0, None);
}
