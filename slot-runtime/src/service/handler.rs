use crate::daemon::RequestHandler;
use crate::domain::{PackageName, SlotId};
use crate::protocol::{Command, Request, Response, ResponsePayload};
use crate::slot_metadata::{SlotDisplayName, SlotSeedMode};

use super::{PreviewService, ServiceError, ServicePlatform};

enum Mutation<'a> {
    Enroll(&'a PackageName, bool),
    Create(&'a PackageName, &'a SlotDisplayName, SlotSeedMode),
    Switch(&'a PackageName, &'a SlotId),
    LaunchCurrent(&'a PackageName, &'a SlotId),
    Rename(&'a PackageName, &'a SlotId, &'a SlotDisplayName),
    Delete(&'a PackageName, &'a SlotId),
    ReconcileAll,
    ReconcilePackage(&'a PackageName),
    Retire(&'a PackageName),
    Rescue(&'a PackageName),
}

impl<P: ServicePlatform> RequestHandler for PreviewService<P> {
    fn handle(&mut self, request: &Request) -> Response {
        let result = if self.recovery_only() && !recovery_command_allowed(request.command()) {
            Err(ServiceError::RecoveryRequired)
        } else {
            self.dispatch(request.command())
        };
        match result {
            Ok(payload) => {
                Response::ok(request.request_id().clone(), payload).unwrap_or_else(|_| {
                    Response::error(request.request_id().clone(), ServiceError::Internal.code())
                })
            }
            Err(error) => Response::error(request.request_id().clone(), error.code()),
        }
    }
}

impl<P: ServicePlatform> PreviewService<P> {
    fn dispatch(&mut self, command: &Command) -> Result<ResponsePayload, ServiceError> {
        match command {
            Command::Probe => self.probe_command(),
            Command::InspectPackage { package } => self.inspect_command(package),
            Command::ListManagedApps => self.managed_apps_command(),
            Command::ListRecoveryTargets => self.recovery_targets_command(),
            Command::StatusPackage { package } => self.status_command(package),
            Command::PackageSnapshot { package } => self.package_snapshot_command(package),
            Command::EnrollPackage {
                package,
                accept_direct_boot_conditional,
            } => self.mutate(&Mutation::Enroll(package, *accept_direct_boot_conditional)),
            Command::CreateSlot {
                package,
                display_name,
                seed_mode,
            } => self.mutate(&Mutation::Create(package, display_name, *seed_mode)),
            Command::ListSlots { package } => self.slots_command(package),
            Command::Switch { package, slot } => self.mutate(&Mutation::Switch(package, slot)),
            Command::LaunchCurrent {
                package,
                expected_slot,
            } => self.mutate(&Mutation::LaunchCurrent(package, expected_slot)),
            Command::RenameSlot {
                package,
                slot,
                display_name,
            } => self.mutate(&Mutation::Rename(package, slot, display_name)),
            Command::DeleteSlot { package, slot } => self.mutate(&Mutation::Delete(package, slot)),
            Command::Reconcile => self.mutate(&Mutation::ReconcileAll),
            Command::ReconcilePackage { package } => {
                self.mutate(&Mutation::ReconcilePackage(package))
            }
            Command::RetirePackage { package } => self.mutate(&Mutation::Retire(package)),
            Command::RescueToBase { package } => self.mutate(&Mutation::Rescue(package)),
        }
    }

    fn mutate(&mut self, operation: &Mutation<'_>) -> Result<ResponsePayload, ServiceError> {
        let guard = self.mutations.clone();
        let _permit = guard.try_acquire().map_err(|_| ServiceError::Busy)?;
        self.require_mutation_capability(operation)?;
        match operation {
            Mutation::Enroll(package, accept) => self.enroll_command(package, *accept),
            Mutation::Create(package, display_name, seed_mode) => {
                self.create_slot_command(package, display_name, *seed_mode)
            }
            Mutation::Switch(package, slot) => self.switch_command(package, slot),
            Mutation::LaunchCurrent(package, expected_slot) => {
                self.launch_current_command(package, expected_slot)
            }
            Mutation::Rename(package, slot, display_name) => {
                self.rename_slot_command(package, slot, display_name)
            }
            Mutation::Delete(package, slot) => self.delete_slot_command(package, slot),
            Mutation::ReconcileAll => self.reconcile_all_command(),
            Mutation::ReconcilePackage(package) => self.reconcile_package_command(package),
            Mutation::Retire(package) => self.retire_package_command(package),
            Mutation::Rescue(package) => self.rescue_command(package),
        }
    }

    fn require_mutation_capability(&self, operation: &Mutation<'_>) -> Result<(), ServiceError> {
        if self.recovery_only() {
            return if matches!(operation, Mutation::Rescue(_)) {
                Ok(())
            } else {
                Err(ServiceError::RecoveryRequired)
            };
        }
        if matches!(
            operation,
            Mutation::ReconcileAll
                | Mutation::ReconcilePackage(_)
                | Mutation::Retire(_)
                | Mutation::Rescue(_)
        ) {
            return Ok(());
        }
        let capability = self.platform.probe()?;
        if !capability.user_unlocked() {
            return Err(ServiceError::UserLocked);
        }
        if !capability.ready() || !capability.ce_de_supported() {
            return Err(ServiceError::UnsupportedDevice);
        }
        Ok(())
    }
}

const fn recovery_command_allowed(command: &Command) -> bool {
    matches!(
        command,
        Command::Probe | Command::ListRecoveryTargets | Command::RescueToBase { .. }
    )
}
