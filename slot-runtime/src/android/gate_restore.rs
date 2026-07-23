use crate::domain::{GateSnapshot, ManagedPackage, PackageObservation, SlotView};
use crate::runtime::PlatformError;

use super::backend::AndroidBackend;
use super::command::{AndroidCommand, CommandError, CommandRunner};
use super::gate_snapshot;
use super::lease::{GateLease, GateLeaseStore, StoredGateLease};
use super::package_observation;
use super::policy::{Stage, ensure_supported, failure};
use super::probe::PackageProbe;

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> AndroidBackend<R, P, L> {
    pub(super) fn restore_gate_state(
        &mut self,
        package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), PlatformError> {
        ensure_supported(package, Stage::RestoreGate)?;
        let lease = self.validated_enrolled_lease(package, Stage::RestoreGate)?;
        if lease.snapshot() != snapshot {
            return Err(failure(Stage::RestoreGate, "gate_snapshot_mismatch"));
        }
        self.ensure_gate(package, Stage::RestoreGate)?;
        self.verify_release_contract(package)?;
        self.runner
            .run(&AndroidCommand::restore_suspended(
                package,
                snapshot.suspended(),
            ))
            .map_err(|error| restore_failure(error, "restore_suspended_failed"))?;
        self.runner
            .run(&AndroidCommand::restore_enabled(
                package,
                snapshot.enabled_state(),
            ))
            .map_err(|error| restore_failure(error, "restore_enabled_failed"))?;
        let restored =
            gate_snapshot::retry(&mut self.probe, package.package_name(), package.user_id())
                .map_err(|error| failure(Stage::RestoreGate, error.code()))?;
        if restored == snapshot {
            Ok(())
        } else {
            Err(failure(Stage::RestoreGate, "restored_gate_mismatch"))
        }
    }

    fn verify_release_contract(&mut self, package: &ManagedPackage) -> Result<(), PlatformError> {
        let observed =
            package_observation::retry(&mut self.probe, package.package_name(), package.user_id())
                .map_err(|error| failure(Stage::RestoreGate, error.code()))?;
        verify_package_contract(package, &observed)?;
        let view = SlotView::new(package.active_slot().clone(), package.active_inodes());
        self.verify_view(package, &view)
            .map_err(|_| failure(Stage::RestoreGate, "final_view_mismatch"))
    }

    pub(super) fn retire_restored_lease(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), PlatformError> {
        ensure_supported(package, Stage::RetireLease)?;
        let lease = match self
            .load_gate_lease_or_contain(package.package_name(), Stage::RetireLease)?
        {
            Some(StoredGateLease::Enrolled(lease)) => lease,
            Some(StoredGateLease::Emergency(_)) => {
                return Err(failure(Stage::RetireLease, "enrolled_gate_lease_required"));
            }
            None => {
                let lease = self
                    .gate_leases
                    .load_retired(package.package_name())
                    .map_err(|_| failure(Stage::RetireLease, "retired_gate_lease_invalid"))?
                    .ok_or_else(|| failure(Stage::RetireLease, "enrolled_gate_lease_required"))?;
                lease
                    .validate_for(package)
                    .map_err(|detail| failure(Stage::RetireLease, detail))?;
                let current = gate_snapshot::retry(
                    &mut self.probe,
                    package.package_name(),
                    package.user_id(),
                )
                .map_err(|error| failure(Stage::RetireLease, error.code()))?;
                return if current == lease.snapshot() {
                    Ok(())
                } else {
                    Err(failure(Stage::RetireLease, "retired_gate_state_mismatch"))
                };
            }
        };
        lease
            .validate_for(package)
            .map_err(|detail| failure(Stage::RetireLease, detail))?;
        let current =
            gate_snapshot::retry(&mut self.probe, package.package_name(), package.user_id())
                .map_err(|error| failure(Stage::RetireLease, error.code()))?;
        if current != lease.snapshot() {
            return Err(failure(
                Stage::RetireLease,
                "gate_state_changed_before_retirement",
            ));
        }
        if self.gate_leases.retire(&lease).is_ok() {
            return Ok(());
        }
        self.contain_untrusted_gate(package.package_name(), Stage::RetireLease)?;
        Err(failure(Stage::RetireLease, "gate_lease_remove_failed"))
    }

    fn validated_enrolled_lease(
        &mut self,
        package: &ManagedPackage,
        stage: Stage,
    ) -> Result<GateLease, PlatformError> {
        let Some(StoredGateLease::Enrolled(lease)) =
            self.load_gate_lease_or_contain(package.package_name(), stage)?
        else {
            return Err(failure(stage, "enrolled_gate_lease_required"));
        };
        lease
            .validate_for(package)
            .map_err(|detail| failure(stage, detail))?;
        Ok(lease)
    }
}

fn verify_package_contract(
    package: &ManagedPackage,
    observed: &PackageObservation,
) -> Result<(), PlatformError> {
    let expected = package.identity();
    let actual = observed.identity();
    if expected.uid() != actual.uid() || expected.signature_sha256() != actual.signature_sha256() {
        return Err(failure(Stage::RestoreGate, "final_identity_changed"));
    }
    if expected.version_code() != actual.version_code()
        || expected.code_path() != actual.code_path()
    {
        return Err(failure(
            Stage::RestoreGate,
            "final_package_metadata_changed",
        ));
    }
    if observed.package_manager_inodes() != package.base_inodes() {
        return Err(failure(Stage::RestoreGate, "final_base_inodes_changed"));
    }
    if observed.pending_install() {
        return Err(failure(Stage::RestoreGate, "final_pending_install"));
    }
    if observed.canonical_inodes() != package.active_inodes()
        || observed.active_process_inodes() != package.active_inodes()
    {
        return Err(failure(Stage::RestoreGate, "final_visible_view_changed"));
    }
    Ok(())
}

fn restore_failure(error: CommandError, fallback: &'static str) -> PlatformError {
    let detail = match error {
        CommandError::IdentityChanged => "final_identity_changed",
        CommandError::PackageStateChanged => "final_package_state_changed",
        CommandError::StartFailed | CommandError::Rejected | CommandError::LaunchEntryNotFound => {
            fallback
        }
    };
    failure(Stage::RestoreGate, detail)
}
