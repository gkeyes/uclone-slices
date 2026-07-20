use crate::domain::{
    GateSnapshot, ManagedPackage, PackageEnabledState, PackageObservation, SlotView,
};
use crate::runtime::{PlatformError, RuntimeBackend};

use super::command::{AndroidCommand, CommandKind, CommandRunner};
use super::gate_snapshot;
use super::lease::{FileGateLeaseStore, GateLease, GateLeaseStore, StoredGateLease};
use super::package_observation;
use super::policy::{Stage, ensure_supported, failure};
use super::probe::PackageProbe;

#[doc = "User-zero, allowlisted Android implementation of the runtime platform boundary."]
#[derive(Debug)]
pub struct AndroidBackend<R, P, L = FileGateLeaseStore> {
    pub(super) runner: R,
    pub(super) probe: P,
    pub(super) gate_leases: L,
}

impl<R, P> AndroidBackend<R, P, FileGateLeaseStore> {
    #[doc = "Creates an adapter from an injected command runner and read-only package probe."]
    pub const fn new(runner: R, probe: P) -> Self {
        Self {
            runner,
            probe,
            gate_leases: FileGateLeaseStore,
        }
    }
}

impl<R, P, L> AndroidBackend<R, P, L> {
    #[doc = "Creates an adapter with an injected deterministic rescue gate-lease store."]
    pub const fn with_gate_lease_store(runner: R, probe: P, gate_leases: L) -> Self {
        Self {
            runner,
            probe,
            gate_leases,
        }
    }

    #[doc = "Returns the injected command runner for deterministic inspection."]
    pub const fn runner(&self) -> &R {
        &self.runner
    }

    #[doc = "Returns the injected package probe for deterministic inspection."]
    pub const fn probe(&self) -> &P {
        &self.probe
    }

    #[doc = "Returns the injected rescue gate-lease store for deterministic inspection."]
    pub const fn gate_lease_store(&self) -> &L {
        &self.gate_leases
    }
}

impl<R: CommandRunner, P: PackageProbe, L> AndroidBackend<R, P, L> {
    pub(super) fn ensure_gate(
        &mut self,
        package: &ManagedPackage,
        stage: Stage,
    ) -> Result<(), PlatformError> {
        ensure_supported(package, stage)?;
        let snapshot =
            gate_snapshot::retry(&mut self.probe, package.package_name(), package.user_id())
                .map_err(|error| failure(stage, error.code()))?;
        if snapshot.enabled_state() == PackageEnabledState::DisabledUser {
            Ok(())
        } else {
            Err(failure(stage, "gate_not_held"))
        }
    }

    pub(super) fn ensure_mount_master(&mut self, stage: Stage) -> Result<(), PlatformError> {
        let proof = self
            .probe
            .mount_namespace_proof()
            .map_err(|error| failure(stage, error.code()))?;
        if proof.is_global() {
            Ok(())
        } else {
            Err(failure(stage, "mount_namespace_not_global"))
        }
    }
}

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> RuntimeBackend
    for AndroidBackend<R, P, L>
{
    fn observe_package(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<PackageObservation, PlatformError> {
        ensure_supported(package, Stage::Observe)?;
        package_observation::retry(&mut self.probe, package.package_name(), package.user_id())
            .map_err(|error| failure(Stage::Observe, error.code()))
    }

    fn capture_gate_snapshot(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<GateSnapshot, PlatformError> {
        ensure_supported(package, Stage::CaptureGate)?;
        match self.load_gate_lease_or_contain(package.package_name(), Stage::CaptureGate)? {
            Some(StoredGateLease::Emergency(lease)) => {
                if lease.package_name() != package.package_name()
                    || lease.user_id() != package.user_id()
                {
                    return Err(failure(Stage::CaptureGate, "gate_lease_identity_mismatch"));
                }
                Err(failure(
                    Stage::CaptureGate,
                    "emergency_gate_confirmation_required",
                ))
            }
            Some(StoredGateLease::Enrolled(lease)) => {
                lease
                    .validate_for(package)
                    .map_err(|detail| failure(Stage::CaptureGate, detail))?;
                Ok(lease.snapshot())
            }
            None => {
                let snapshot = gate_snapshot::retry(
                    &mut self.probe,
                    package.package_name(),
                    package.user_id(),
                )
                .map_err(|error| failure(Stage::CaptureGate, error.code()))?;
                self.gate_leases
                    .persist_enrolled(&GateLease::capture(package, snapshot))
                    .map_err(|_| failure(Stage::CaptureGate, "gate_lease_persist_failed"))?;
                Ok(snapshot)
            }
        }
    }

    fn acquire_gate(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        ensure_supported(package, Stage::AcquireGate)?;
        let lease =
            match self.load_gate_lease_or_contain(package.package_name(), Stage::AcquireGate)? {
                Some(StoredGateLease::Enrolled(lease)) => lease,
                Some(StoredGateLease::Emergency(_)) | None => {
                    return Err(failure(Stage::AcquireGate, "enrolled_gate_lease_required"));
                }
            };
        lease
            .validate_for(package)
            .map_err(|detail| failure(Stage::AcquireGate, detail))?;
        let current =
            gate_snapshot::retry(&mut self.probe, package.package_name(), package.user_id())
                .map_err(|error| failure(Stage::AcquireGate, error.code()))?;
        let held = GateSnapshot::new(
            PackageEnabledState::DisabledUser,
            lease.snapshot().suspended(),
        );
        if current == held {
            return Ok(());
        }
        if current != lease.snapshot() {
            return Err(failure(
                Stage::AcquireGate,
                "gate_state_changed_before_acquire",
            ));
        }
        self.runner
            .run(&AndroidCommand::package(
                CommandKind::DisableUser,
                package.package_name(),
            ))
            .map_err(|_| failure(Stage::AcquireGate, "disable_user_failed"))
    }

    fn verify_gate_held(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.ensure_gate(package, Stage::VerifyGate)
    }

    fn quiesce_processes(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.ensure_gate(package, Stage::Quiesce)?;
        self.runner
            .run(&AndroidCommand::package(
                CommandKind::ForceStop,
                package.package_name(),
            ))
            .map_err(|_| failure(Stage::Quiesce, "force_stop_failed"))?;
        self.wait_for_processes_to_exit(package.package_name(), package.user_id())
    }

    fn apply_slot_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        self.apply_view(package, view)
    }

    fn verify_slot_view(
        &mut self,
        package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        self.verify_view(package, view)
    }

    fn restore_gate(
        &mut self,
        package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), PlatformError> {
        self.restore_gate_state(package, snapshot)
    }

    fn retire_gate_lease(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.retire_restored_lease(package)
    }
}
