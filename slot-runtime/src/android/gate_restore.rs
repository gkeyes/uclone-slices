use crate::domain::{GateSnapshot, ManagedPackage};
use crate::runtime::PlatformError;

use super::backend::AndroidBackend;
use super::command::{AndroidCommand, CommandKind, CommandRunner};
use super::gate_snapshot;
use super::lease::{GateLease, GateLeaseStore, StoredGateLease};
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
        self.runner
            .run(&AndroidCommand::package(
                CommandKind::RestoreSuspended(snapshot.suspended()),
                package.package_name(),
            ))
            .map_err(|_| failure(Stage::RestoreGate, "restore_suspended_failed"))?;
        self.runner
            .run(&AndroidCommand::package(
                CommandKind::RestoreEnabled(snapshot.enabled_state()),
                package.package_name(),
            ))
            .map_err(|_| failure(Stage::RestoreGate, "restore_enabled_failed"))?;
        let restored =
            gate_snapshot::retry(&mut self.probe, package.package_name(), package.user_id())
                .map_err(|error| failure(Stage::RestoreGate, error.code()))?;
        if restored == snapshot {
            Ok(())
        } else {
            Err(failure(Stage::RestoreGate, "restored_gate_mismatch"))
        }
    }

    pub(super) fn retire_restored_lease(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), PlatformError> {
        ensure_supported(package, Stage::RetireLease)?;
        let lease = self.validated_enrolled_lease(package, Stage::RetireLease)?;
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
