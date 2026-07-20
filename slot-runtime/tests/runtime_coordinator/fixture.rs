#![allow(
    clippy::redundant_pub_crate,
    reason = "the private integration fixture module exports only to its test root"
)]

use crate::support::{ManagedFixture, PackageContract, managed_with_contract};
use tempfile::tempdir;
use uclone_slot_runtime::domain::{
    AppIdentity, BootId, CommitNonce, DataInodes, GateSnapshot, ManagedPackage,
    PackageEnabledState, PackageObservation, SlotId, SlotView, TransactionId,
};
use uclone_slot_runtime::journal::JournalStore;
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::registry::RegistryStore;
use uclone_slot_runtime::runtime::{
    FaultInjector, PlatformError, RuntimeBackend, RuntimeStores, SwitchCoordinator, SwitchMetadata,
    SwitchRequest,
};

#[derive(Debug)]
pub(super) struct FakeBackend {
    pub(super) observation: PackageObservation,
    pub(super) late_observation: Option<PackageObservation>,
    observation_count: u32,
    snapshot: GateSnapshot,
    pub(super) current: SlotView,
    pub(super) gate_held: bool,
    pub(super) lease_retired: bool,
    pub(super) failure: FailureMode,
    gate_acquire_calls: u32,
    gate_verifications: u32,
    pub(super) mutations: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "shared integration fixture variants are exercised by different test roots"
)]
pub(super) enum FailureMode {
    None,
    GateAcquire,
    GateAcquireAfterMutation,
    TargetVerification,
    RollbackAndContainment,
    GateLeaseRetirement,
}

impl FakeBackend {
    pub(super) fn healthy(managed: &ManagedPackage) -> Self {
        let inodes = managed.active_inodes();
        Self {
            observation: PackageObservation::new(
                managed.identity().clone(),
                managed.base_inodes(),
                inodes,
                inodes,
                false,
            ),
            late_observation: None,
            observation_count: 0,
            snapshot: GateSnapshot::new(PackageEnabledState::Default, false),
            current: SlotView::new(managed.active_slot().clone(), inodes),
            gate_held: false,
            lease_retired: false,
            failure: FailureMode::None,
            gate_acquire_calls: 0,
            gate_verifications: 0,
            mutations: Vec::new(),
        }
    }
}

impl RuntimeBackend for FakeBackend {
    fn observe_package(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<PackageObservation, PlatformError> {
        self.observation_count += 1;
        if self.observation_count > 1 {
            return Ok(self
                .late_observation
                .clone()
                .unwrap_or_else(|| self.observation.clone()));
        }
        Ok(self.observation.clone())
    }

    fn capture_gate_snapshot(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<GateSnapshot, PlatformError> {
        Ok(self.snapshot)
    }

    fn acquire_gate(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.gate_acquire_calls += 1;
        if self.failure == FailureMode::GateAcquire {
            return Err(PlatformError::AcquireGate {
                detail: "injected_acquire_failure".to_owned(),
            });
        }
        if !self.gate_held {
            self.gate_held = true;
            self.mutations.push("gate_acquired");
        }
        if self.failure == FailureMode::GateAcquireAfterMutation && self.gate_acquire_calls == 1 {
            return Err(PlatformError::AcquireGate {
                detail: "injected_acknowledgement_loss".to_owned(),
            });
        }
        Ok(())
    }

    fn verify_gate_held(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.gate_verifications += 1;
        if self.failure == FailureMode::RollbackAndContainment && self.gate_verifications > 2 {
            return Err(PlatformError::VerifyGateHeld {
                detail: "injected_containment_failure".to_owned(),
            });
        }
        if self.gate_held {
            Ok(())
        } else {
            Err(PlatformError::VerifyGateHeld {
                detail: "gate_not_held".to_owned(),
            })
        }
    }

    fn quiesce_processes(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.mutations.push("processes_quiesced");
        Ok(())
    }

    fn apply_slot_view(
        &mut self,
        _package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        self.current = view.clone();
        self.mutations.push("view_applied");
        Ok(())
    }

    fn verify_slot_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        if matches!(
            self.failure,
            FailureMode::TargetVerification | FailureMode::RollbackAndContainment
        ) && view.slot_id() != package.active_slot()
        {
            return Err(PlatformError::view_verification("injected_target_mismatch"));
        }
        if self.failure == FailureMode::RollbackAndContainment
            && view.slot_id() == package.active_slot()
        {
            return Err(PlatformError::view_verification(
                "injected_previous_mismatch",
            ));
        }
        assert_eq!(&self.current, view);
        Ok(())
    }

    fn restore_gate(
        &mut self,
        _package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), PlatformError> {
        assert_eq!(snapshot, self.snapshot);
        self.gate_held = false;
        self.mutations.push("gate_restored");
        Ok(())
    }

    fn retire_gate_lease(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        if self.failure == FailureMode::GateLeaseRetirement {
            return Err(PlatformError::RetireGateLease {
                detail: "injected_lease_retirement_failure".to_owned(),
            });
        }
        self.lease_retired = true;
        self.mutations.push("gate_lease_retired");
        Ok(())
    }
}

pub(super) fn request(transaction: &str) -> SwitchRequest {
    let base = DataInodes::new(100, 200).unwrap();
    let managed = managed_with_contract(ManagedFixture::new(
        base,
        SlotView::new(SlotId::base(), base),
        PackageContract::new(
            AppIdentity::new(
                10_321,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                1,
                "/data/app/slotprobe/base.apk",
            )
            .unwrap(),
            LifecycleState::Normal,
        ),
    ));
    SwitchRequest::new(
        managed,
        SlotView::new(
            SlotId::parse("work").unwrap(),
            DataInodes::new(300, 400).unwrap(),
        ),
        SwitchMetadata::new(
            TransactionId::parse(transaction).unwrap(),
            CommitNonce::parse("nonce-runtime-0001").unwrap(),
            BootId::parse("boot-runtime-0001").unwrap(),
        ),
    )
}

pub(super) fn coordinator(
    backend: FakeBackend,
    faults: FaultInjector,
) -> (tempfile::TempDir, SwitchCoordinator<FakeBackend>) {
    let root = tempdir().unwrap();
    crate::support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let stores = RuntimeStores::new(journal, registry);
    (
        root,
        SwitchCoordinator::with_fault_injector(backend, stores, faults),
    )
}
