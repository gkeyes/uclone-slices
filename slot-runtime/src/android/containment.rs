use crate::domain::{PackageEnabledState, PackageName, UserId};
use crate::runtime::PlatformError;

use super::backend::AndroidBackend;
use super::command::{AndroidCommand, CommandKind, CommandRunner};
use super::gate_snapshot;
use super::lease::{GateLeaseStore, StoredGateLease};
use super::policy::{Stage, failure};
use super::probe::PackageProbe;

impl<R: CommandRunner, P: PackageProbe, L: GateLeaseStore> AndroidBackend<R, P, L> {
    pub(super) fn load_gate_lease_or_contain(
        &mut self,
        package: &PackageName,
        stage: Stage,
    ) -> Result<Option<StoredGateLease>, PlatformError> {
        if let Ok(lease) = self.gate_leases.load(package) {
            Ok(lease)
        } else {
            self.contain_untrusted_gate(package, stage)?;
            Err(failure(stage, "gate_lease_load_failed"))
        }
    }

    pub(super) fn contain_untrusted_gate(
        &mut self,
        package: &PackageName,
        stage: Stage,
    ) -> Result<(), PlatformError> {
        self.runner
            .run(&AndroidCommand::package(CommandKind::DisableUser, package))
            .map_err(|_| failure(stage, "disable_user_failed"))?;
        let held = gate_snapshot::retry(&mut self.probe, package, UserId::PRIMARY)
            .map_err(|error| failure(stage, error.code()))?;
        if held.enabled_state() != PackageEnabledState::DisabledUser {
            return Err(failure(stage, "gate_not_held"));
        }
        self.runner
            .run(&AndroidCommand::package(CommandKind::ForceStop, package))
            .map_err(|_| failure(stage, "force_stop_failed"))?;
        let count = self
            .probe
            .running_process_count(package, UserId::PRIMARY)
            .map_err(|error| failure(stage, error.code()))?;
        if count == 0 {
            Ok(())
        } else {
            Err(failure(stage, "processes_still_running"))
        }
    }
}
