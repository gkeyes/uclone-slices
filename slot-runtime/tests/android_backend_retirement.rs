#![doc = "Two-phase Android gate-lease retirement validation coverage."]
#![allow(clippy::unwrap_used, reason = "validated test fixtures")]

use std::collections::VecDeque;

use uclone_slot_runtime::android::{
    AndroidBackend, AndroidCommand, CommandError, CommandRunner, EmergencyGateLease, GateLease,
    GateLeaseError, GateLeaseStore, MountNamespaceProof, PackageProbe, ProbeError, StoredGateLease,
    ViewProof,
};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, PackageObservation, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::reconcile::RecoveryBackend;
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};

#[derive(Debug, Default)]
struct AcceptingRunner;

impl CommandRunner for AcceptingRunner {
    fn run(&mut self, _command: &AndroidCommand) -> Result<(), CommandError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
struct MemoryLeaseStore {
    active: Option<StoredGateLease>,
    retired: Option<GateLease>,
}

impl GateLeaseStore for MemoryLeaseStore {
    fn artifact_exists(&mut self, _package: &PackageName) -> Result<bool, GateLeaseError> {
        Ok(self.active.is_some())
    }

    fn load(&mut self, _package: &PackageName) -> Result<Option<StoredGateLease>, GateLeaseError> {
        Ok(self.active.clone())
    }

    fn load_retired(
        &mut self,
        _package: &PackageName,
    ) -> Result<Option<GateLease>, GateLeaseError> {
        Ok(self.retired.clone())
    }

    fn persist_emergency(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        self.active = Some(StoredGateLease::Emergency(lease.clone()));
        Ok(())
    }

    fn mark_emergency_held(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError> {
        if self.active != Some(StoredGateLease::Emergency(lease.clone())) {
            return Err(GateLeaseError::InvalidArtifact);
        }
        self.active = Some(StoredGateLease::Emergency(lease.held()));
        Ok(())
    }

    fn persist_enrolled(&mut self, lease: &GateLease) -> Result<(), GateLeaseError> {
        self.active = Some(StoredGateLease::Enrolled(lease.clone()));
        Ok(())
    }

    fn confirm_enrollment(&mut self, lease: &GateLease) -> Result<(), GateLeaseError> {
        if !matches!(
            self.active.as_ref(),
            Some(StoredGateLease::Emergency(existing))
                if existing.phase() == uclone_slot_runtime::android::EmergencyGatePhase::Held
                    && existing.snapshot() == lease.snapshot()
        ) {
            return Err(GateLeaseError::InvalidArtifact);
        }
        self.active = Some(StoredGateLease::Enrolled(lease.clone()));
        Ok(())
    }

    fn retire(&mut self, expected: &GateLease) -> Result<(), GateLeaseError> {
        if self.active != Some(StoredGateLease::Enrolled(expected.clone())) {
            return Err(GateLeaseError::InvalidArtifact);
        }
        self.active = None;
        self.retired = Some(expected.clone());
        Ok(())
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

fn managed(base: DataInodes) -> ManagedPackage {
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

fn backend(
    gates: VecDeque<GateSnapshot>,
) -> AndroidBackend<AcceptingRunner, GateProbe, MemoryLeaseStore> {
    AndroidBackend::with_gate_lease_store(
        AcceptingRunner,
        GateProbe(gates),
        MemoryLeaseStore::default(),
    )
}

#[test]
fn preliminary_emergency_lease_cannot_be_retired() {
    let package = managed(DataInodes::new(101, 202).unwrap());
    let original = GateSnapshot::new(PackageEnabledState::Default, false);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let mut backend = backend(VecDeque::from([original, original, held]));
    backend.emergency_gate(package.package_name()).unwrap();

    let error = backend.retire_gate_lease(&package).unwrap_err();

    assert_eq!(
        error,
        PlatformError::RetireGateLease {
            detail: "enrolled_gate_lease_required".to_owned(),
        }
    );
    assert!(matches!(
        backend.gate_lease_store().active,
        Some(StoredGateLease::Emergency(_))
    ));
}

#[test]
fn enrolled_lease_with_different_base_anchors_cannot_be_retired() {
    let package = managed(DataInodes::new(101, 202).unwrap());
    let mismatched = managed(DataInodes::new(303, 404).unwrap());
    let original = GateSnapshot::new(PackageEnabledState::Enabled, true);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, true);
    let mut backend = backend(VecDeque::from([original, original, held, held, held]));
    backend.emergency_gate(package.package_name()).unwrap();
    backend.confirm_emergency_gate(&package).unwrap();

    let error = backend.retire_gate_lease(&mismatched).unwrap_err();

    assert_eq!(
        error,
        PlatformError::RetireGateLease {
            detail: "gate_lease_base_mismatch".to_owned(),
        }
    );
    assert!(matches!(
        backend.gate_lease_store().active,
        Some(StoredGateLease::Enrolled(_))
    ));
}

#[test]
fn changed_package_state_prevents_retirement_and_preserves_the_lease() {
    let package = managed(DataInodes::new(101, 202).unwrap());
    let original = GateSnapshot::new(PackageEnabledState::Enabled, true);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, true);
    let changed = GateSnapshot::new(PackageEnabledState::Enabled, false);
    let mut backend = backend(VecDeque::from([
        original, original, held, held, held, changed,
    ]));
    backend.emergency_gate(package.package_name()).unwrap();
    backend.confirm_emergency_gate(&package).unwrap();

    let error = backend.retire_gate_lease(&package).unwrap_err();

    assert_eq!(
        error,
        PlatformError::RetireGateLease {
            detail: "gate_state_changed_before_retirement".to_owned(),
        }
    );
    assert!(matches!(
        backend.gate_lease_store().active,
        Some(StoredGateLease::Enrolled(_))
    ));
}

#[test]
fn matching_retired_lease_is_an_idempotent_exact_state_proof() {
    let package = managed(DataInodes::new(101, 202).unwrap());
    let original = GateSnapshot::new(PackageEnabledState::Enabled, true);
    let mut backend = backend(VecDeque::from([original, original, original]));
    backend.capture_gate_snapshot(&package).unwrap();
    backend.retire_gate_lease(&package).unwrap();

    backend.retire_gate_lease(&package).unwrap();

    assert!(backend.gate_lease_store().active.is_none());
    assert!(backend.gate_lease_store().retired.is_some());
}
