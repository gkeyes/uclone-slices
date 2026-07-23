use crate::domain::ManagedPackage;
use crate::launch::{AppLaunchBackend, LaunchDisposition};

use super::backend::AndroidBackend;
use super::command::{AndroidCommand, CommandError, CommandRunner};

impl<R, P, L> AppLaunchBackend for AndroidBackend<R, P, L>
where
    R: CommandRunner,
    P: core::fmt::Debug,
    L: core::fmt::Debug,
{
    fn launch_package(&mut self, package: &ManagedPackage) -> LaunchDisposition {
        match self.runner.run(&AndroidCommand::launch(package)) {
            Ok(()) => LaunchDisposition::Launched,
            Err(CommandError::LaunchEntryNotFound) => LaunchDisposition::EntryNotFound,
            Err(CommandError::IdentityChanged) => LaunchDisposition::IdentityChanged,
            Err(CommandError::PackageStateChanged) => LaunchDisposition::PackageStateChanged,
            Err(CommandError::StartFailed | CommandError::Rejected) => LaunchDisposition::Failed,
        }
    }
}
