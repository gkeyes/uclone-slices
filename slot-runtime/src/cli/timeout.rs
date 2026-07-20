use std::time::Duration;

use super::CliCommand;
use crate::protocol::Command;

const RESCUE_TIMEOUT: Duration = Duration::from_secs(10);
const CREATE_TIMEOUT: Duration = Duration::from_mins(30);
const MUTATION_TIMEOUT: Duration = Duration::from_mins(2);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) const fn for_command(command: &CliCommand) -> Duration {
    match command {
        CliCommand::Rescue { .. } => RESCUE_TIMEOUT,
        CliCommand::Create { .. } => CREATE_TIMEOUT,
        CliCommand::Enroll { .. }
        | CliCommand::Switch { .. }
        | CliCommand::Rename { .. }
        | CliCommand::Delete { .. }
        | CliCommand::Reconcile { .. }
        | CliCommand::Retire { .. } => MUTATION_TIMEOUT,
        CliCommand::Rpc
        | CliCommand::Probe
        | CliCommand::Inspect { .. }
        | CliCommand::Apps
        | CliCommand::Status { .. }
        | CliCommand::Slots { .. } => READ_TIMEOUT,
    }
}

pub(super) const fn for_request(command: &Command) -> Duration {
    match command {
        Command::RescueToBase { .. } => RESCUE_TIMEOUT,
        Command::CreateSlot { .. } => CREATE_TIMEOUT,
        Command::EnrollPackage { .. }
        | Command::Switch { .. }
        | Command::RenameSlot { .. }
        | Command::DeleteSlot { .. }
        | Command::Reconcile
        | Command::ReconcilePackage { .. }
        | Command::RetirePackage { .. } => MUTATION_TIMEOUT,
        Command::Probe
        | Command::InspectPackage { .. }
        | Command::ListManagedApps
        | Command::StatusPackage { .. }
        | Command::ListSlots { .. } => READ_TIMEOUT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rescue_is_short_and_ordinary_work_is_long_but_bounded() {
        let rescue = CliCommand::Rescue {
            package: crate::target::PACKAGE.to_owned(),
            to_base: true,
        };
        assert_eq!(for_command(&rescue), Duration::from_secs(10));
        assert_eq!(for_command(&CliCommand::Probe), Duration::from_secs(30));
    }
}
