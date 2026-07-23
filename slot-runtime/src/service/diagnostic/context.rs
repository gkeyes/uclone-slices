use crate::domain::{PackageName, SlotId};
use crate::protocol::{Command, Request, RequestId};

use super::model::DiagnosticCommand;
use super::model::OperationPhase;

#[doc = "Non-wire request context; display names and paths are excluded."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationContext {
    request_id: RequestId,
    command: DiagnosticCommand,
    package: Option<PackageName>,
    slot: Option<SlotId>,
    phase: OperationPhase,
}

impl OperationContext {
    #[doc = "Builds safe context from a validated request."]
    pub fn from_request(request: &Request) -> Self {
        let (command, package, slot) = command_parts(request.command());
        Self {
            request_id: request.request_id().clone(),
            command,
            package,
            slot,
            phase: OperationPhase::RequestReceived,
        }
    }

    #[doc = "Returns the bounded request identifier."]
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }
    #[doc = "Returns the typed command name."]
    pub const fn command(&self) -> DiagnosticCommand {
        self.command
    }
    #[doc = "Returns the targeted package, when present."]
    pub const fn package(&self) -> Option<&PackageName> {
        self.package.as_ref()
    }
    #[doc = "Returns the targeted slot, when present."]
    pub const fn slot(&self) -> Option<&SlotId> {
        self.slot.as_ref()
    }
    #[doc = "Returns the latest internal phase."]
    pub const fn phase(&self) -> OperationPhase {
        self.phase
    }

    pub(crate) const fn set_phase(&mut self, phase: OperationPhase) {
        self.phase = phase;
    }
}

fn command_parts(command: &Command) -> (DiagnosticCommand, Option<PackageName>, Option<SlotId>) {
    match command {
        Command::Probe => (DiagnosticCommand::Probe, None, None),
        Command::InspectPackage { package } => {
            package_fields(DiagnosticCommand::InspectPackage, package)
        }
        Command::ListManagedApps => (DiagnosticCommand::ListManagedApps, None, None),
        Command::ListRecoveryTargets => (DiagnosticCommand::ListRecoveryTargets, None, None),
        Command::StatusPackage { package } => {
            package_fields(DiagnosticCommand::StatusPackage, package)
        }
        Command::PackageSnapshot { package } => {
            package_fields(DiagnosticCommand::PackageSnapshot, package)
        }
        Command::EnrollPackage { package, .. } => {
            package_fields(DiagnosticCommand::EnrollPackage, package)
        }
        Command::CreateSlot { package, .. } => {
            package_fields(DiagnosticCommand::CreateSlot, package)
        }
        Command::ListSlots { package } => package_fields(DiagnosticCommand::ListSlots, package),
        Command::Switch { package, slot } => slot_fields(DiagnosticCommand::Switch, package, slot),
        Command::LaunchCurrent {
            package,
            expected_slot,
        } => slot_fields(DiagnosticCommand::LaunchCurrent, package, expected_slot),
        Command::RenameSlot { package, slot, .. } => {
            slot_fields(DiagnosticCommand::RenameSlot, package, slot)
        }
        Command::DeleteSlot { package, slot } => {
            slot_fields(DiagnosticCommand::DeleteSlot, package, slot)
        }
        Command::Reconcile => (DiagnosticCommand::Reconcile, None, None),
        Command::ReconcilePackage { package } => {
            package_fields(DiagnosticCommand::ReconcilePackage, package)
        }
        Command::RetirePackage { package } => {
            package_fields(DiagnosticCommand::RetirePackage, package)
        }
        Command::RescueToBase { package } => {
            package_fields(DiagnosticCommand::RescueToBase, package)
        }
    }
}

fn package_fields(
    name: DiagnosticCommand,
    value: &PackageName,
) -> (DiagnosticCommand, Option<PackageName>, Option<SlotId>) {
    (name, Some(value.clone()), None)
}

fn slot_fields(
    name: DiagnosticCommand,
    package: &PackageName,
    value: &SlotId,
) -> (DiagnosticCommand, Option<PackageName>, Option<SlotId>) {
    (name, Some(package.clone()), Some(value.clone()))
}
