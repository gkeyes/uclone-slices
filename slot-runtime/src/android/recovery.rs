use crate::domain::{GateSnapshot, ManagedPackage, PackageEnabledState, PackageName, UserId};
use crate::reconcile::{NativeBaseRecoveryBackend, RecoveryBackend};
use crate::runtime::PlatformError;

use super::backend::AndroidBackend;
use super::command::{AndroidCommand, CommandKind, CommandRunner};
use super::gate_snapshot;
use super::lease::{
    EmergencyGateLease, EmergencyGatePhase, GateLease, GateLeaseStore, StoredGateLease,
};
use super::policy::{Stage, ensure_package_supported, ensure_supported, failure};
use super::probe::{PackageProbe, ProbeError};

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> RecoveryBackend
    for AndroidBackend<R, P, L>
{
    fn emergency_gate_if_leased(
        &mut self,
        package: &PackageName,
    ) -> Result<Option<GateSnapshot>, PlatformError> {
        ensure_package_supported(package, Stage::CaptureGate)?;
        if matches!(self.gate_leases.artifact_exists(package), Ok(false)) {
            return Ok(None);
        }
        let lease = self
            .load_gate_lease_or_contain(package, Stage::CaptureGate)?
            .ok_or_else(|| failure(Stage::CaptureGate, "gate_lease_disappeared"))?;
        validate_preliminary_identity(&lease, package)?;
        match lease {
            StoredGateLease::Emergency(lease) => {
                let snapshot = lease.snapshot();
                self.hold_emergency(package, &lease)?;
                Ok(Some(snapshot))
            }
            StoredGateLease::Enrolled(lease) => {
                let snapshot = lease.snapshot();
                self.hold_known_gate(package, snapshot, true)?;
                Ok(Some(snapshot))
            }
        }
    }

    fn emergency_gate(&mut self, package: &PackageName) -> Result<GateSnapshot, PlatformError> {
        ensure_package_supported(package, Stage::CaptureGate)?;
        if self.gate_leases.artifact_exists(package) == Ok(false) {
            let snapshot = gate_snapshot::retry(&mut self.probe, package, UserId::PRIMARY)
                .map_err(|error| failure(Stage::CaptureGate, error.code()))?;
            let lease = EmergencyGateLease::capture(package, snapshot);
            self.gate_leases
                .persist_emergency(&lease)
                .map_err(|_| failure(Stage::CaptureGate, "gate_lease_persist_failed"))?;
            self.hold_emergency(package, &lease)?;
            Ok(snapshot)
        } else {
            let lease = self
                .load_gate_lease_or_contain(package, Stage::CaptureGate)?
                .ok_or_else(|| failure(Stage::CaptureGate, "gate_lease_disappeared"))?;
            validate_preliminary_identity(&lease, package)?;
            match lease {
                StoredGateLease::Emergency(lease) => {
                    let snapshot = lease.snapshot();
                    self.hold_emergency(package, &lease)?;
                    Ok(snapshot)
                }
                StoredGateLease::Enrolled(lease) => {
                    let snapshot = lease.snapshot();
                    self.hold_known_gate(package, snapshot, true)?;
                    Ok(snapshot)
                }
            }
        }
    }

    fn confirm_emergency_gate(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        ensure_supported(package, Stage::VerifyGate)?;
        self.ensure_gate(package, Stage::VerifyGate)?;
        self.force_stop_and_prove(package.package_name())?;
        match self.load_gate_lease_or_contain(package.package_name(), Stage::VerifyGate)? {
            Some(StoredGateLease::Emergency(lease)) => {
                if lease.package_name() != package.package_name()
                    || lease.user_id() != package.user_id()
                {
                    return Err(failure(Stage::VerifyGate, "gate_lease_identity_mismatch"));
                }
                if lease.phase() != EmergencyGatePhase::Held {
                    return Err(failure(Stage::VerifyGate, "gate_lease_not_held"));
                }
                self.gate_leases
                    .confirm_enrollment(&GateLease::capture(package, lease.snapshot()))
                    .map_err(|_| failure(Stage::VerifyGate, "gate_lease_confirm_failed"))?;
            }
            Some(StoredGateLease::Enrolled(lease))
                if lease.package_name() == package.package_name()
                    && lease.user_id() == package.user_id()
                    && lease.base_inodes() == package.base_inodes() => {}
            Some(StoredGateLease::Enrolled(_)) => {
                return Err(failure(Stage::VerifyGate, "gate_lease_base_mismatch"));
            }
            None => return Err(failure(Stage::VerifyGate, "gate_lease_missing")),
        }
        self.ensure_gate(package, Stage::VerifyGate)
    }

    fn user0_unlocked(&mut self) -> Result<bool, PlatformError> {
        self.probe.user0_unlocked().map_err(|error| {
            failure(
                Stage::Observe,
                match error {
                    ProbeError::Unavailable => "user0_unlock_probe_unavailable",
                    ProbeError::InvalidResponse => "user0_unlock_probe_invalid_response",
                },
            )
        })
    }

    fn verify_native_base(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        self.verify_base_without_mutation(package)
    }
}

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> NativeBaseRecoveryBackend
    for AndroidBackend<R, P, L>
{
    fn restore_native_base_unconditionally(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), PlatformError> {
        self.restore_native_base(package)
    }

    fn verify_exact_gate_state(
        &mut self,
        package: &ManagedPackage,
        expected: GateSnapshot,
    ) -> Result<bool, PlatformError> {
        ensure_supported(package, Stage::VerifyGate)?;
        gate_snapshot::retry(&mut self.probe, package.package_name(), package.user_id())
            .map(|actual| actual == expected)
            .map_err(|error| failure(Stage::VerifyGate, error.code()))
    }
}

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> AndroidBackend<R, P, L> {
    fn hold_emergency(
        &mut self,
        package: &PackageName,
        lease: &EmergencyGateLease,
    ) -> Result<(), PlatformError> {
        let allow_already_held = lease.phase() == EmergencyGatePhase::Held;
        self.hold_known_gate(package, lease.snapshot(), allow_already_held)?;
        if lease.phase() == EmergencyGatePhase::Prepared {
            self.gate_leases
                .mark_emergency_held(lease)
                .map_err(|_| failure(Stage::VerifyGate, "gate_lease_hold_commit_failed"))?;
        }
        Ok(())
    }

    fn hold_known_gate(
        &mut self,
        package: &PackageName,
        expected_snapshot: GateSnapshot,
        allow_already_held: bool,
    ) -> Result<(), PlatformError> {
        let current = gate_snapshot::retry(&mut self.probe, package, UserId::PRIMARY)
            .map_err(|error| failure(Stage::AcquireGate, error.code()))?;
        let disabled_with_expected_suspension = current.enabled_state()
            == PackageEnabledState::DisabledUser
            && current.suspended() == expected_snapshot.suspended();
        if current != expected_snapshot && !allow_already_held && disabled_with_expected_suspension
        {
            self.contain_untrusted_gate(package, Stage::AcquireGate)?;
            return Err(failure(
                Stage::AcquireGate,
                "prepared_gate_ambiguous_disabled",
            ));
        }
        if current != expected_snapshot
            && !(allow_already_held && disabled_with_expected_suspension)
        {
            return Err(failure(
                Stage::AcquireGate,
                "gate_state_changed_before_emergency_acquire",
            ));
        }
        self.runner
            .run(&AndroidCommand::package(CommandKind::DisableUser, package))
            .map_err(|_| failure(Stage::AcquireGate, "disable_user_failed"))?;
        verify_emergency_gate(&mut self.probe, package)?;
        self.force_stop_and_prove(package)
    }

    fn force_stop_and_prove(&mut self, package: &PackageName) -> Result<(), PlatformError> {
        self.runner
            .run(&AndroidCommand::package(CommandKind::ForceStop, package))
            .map_err(|_| failure(Stage::Quiesce, "force_stop_failed"))?;
        self.wait_for_processes_to_exit(package, UserId::PRIMARY)
    }
}

fn validate_preliminary_identity(
    lease: &StoredGateLease,
    package: &PackageName,
) -> Result<(), PlatformError> {
    if lease.package_name() != package || lease.user_id() != UserId::PRIMARY {
        return Err(failure(Stage::CaptureGate, "gate_lease_identity_mismatch"));
    }
    Ok(())
}

fn verify_emergency_gate<P: PackageProbe>(
    probe: &mut P,
    package: &PackageName,
) -> Result<(), PlatformError> {
    let held = gate_snapshot::retry(probe, package, UserId::PRIMARY)
        .map_err(|error| failure(Stage::VerifyGate, error.code()))?;
    if held.enabled_state() != PackageEnabledState::DisabledUser {
        return Err(failure(Stage::VerifyGate, "gate_not_held"));
    }
    Ok(())
}
