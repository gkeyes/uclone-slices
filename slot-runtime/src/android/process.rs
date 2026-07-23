use std::ffi::OsStr;
use std::process::Command;

use crate::domain::PackageEnabledState;

use super::command::{AndroidCommand, CommandError, CommandKind, CommandRunner, CommandSpec};

#[derive(Debug, Clone, Copy)]
enum Executable {
    Cmd,
    ActivityManager,
    Mount,
    Unmount,
}

impl Executable {
    const fn path(self) -> &'static str {
        match self {
            Self::Cmd => "/system/bin/cmd",
            Self::ActivityManager => "/system/bin/am",
            Self::Mount => "/system/bin/mount",
            Self::Unmount => "/system/bin/umount",
        }
    }
}

#[doc = "Production runner that invokes only fixed Android executables with typed argv."]
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError> {
        let package = OsStr::new(command.package_name().as_str());
        match &command.0 {
            CommandSpec::Package { kind, .. } => run_package_command(*kind, package),
            CommandSpec::Launch { .. } | CommandSpec::ContractMutation { .. } => {
                Err(CommandError::Rejected)
            }
            CommandSpec::Bind { source, target, .. } => run_process(
                Executable::Mount,
                &[OsStr::new("--bind"), source.as_os_str(), target.as_os_str()],
            ),
            CommandSpec::Unmount { target, .. } => {
                run_process(Executable::Unmount, &[target.as_os_str()])
            }
        }
    }
}

fn run_package_command(kind: CommandKind, package: &OsStr) -> Result<(), CommandError> {
    let user = OsStr::new("0");
    match kind {
        CommandKind::DisableUser => run_process(
            Executable::Cmd,
            &[
                OsStr::new("package"),
                OsStr::new("disable-user"),
                OsStr::new("--user"),
                user,
                package,
            ],
        ),
        CommandKind::ForceStop => run_process(
            Executable::ActivityManager,
            &[
                OsStr::new("force-stop"),
                OsStr::new("--user"),
                user,
                package,
            ],
        ),
        CommandKind::RestoreEnabled(state) => run_process(
            Executable::Cmd,
            &[
                OsStr::new("package"),
                OsStr::new(enabled_verb(state)),
                OsStr::new("--user"),
                user,
                package,
            ],
        ),
        CommandKind::RestoreSuspended(suspended) => run_process(
            Executable::Cmd,
            &[
                OsStr::new("package"),
                OsStr::new(if suspended { "suspend" } else { "unsuspend" }),
                OsStr::new("--user"),
                user,
                package,
            ],
        ),
        CommandKind::LaunchPackage | CommandKind::Bind(_) | CommandKind::Unmount(_) => {
            Err(CommandError::Rejected)
        }
    }
}

const fn enabled_verb(state: PackageEnabledState) -> &'static str {
    match state {
        PackageEnabledState::Default => "default-state",
        PackageEnabledState::Enabled => "enable",
        PackageEnabledState::Disabled => "disable",
        PackageEnabledState::DisabledUser => "disable-user",
        PackageEnabledState::DisabledUntilUsed => "disable-until-used",
    }
}

fn run_process(executable: Executable, arguments: &[&OsStr]) -> Result<(), CommandError> {
    let output = Command::new(executable.path())
        .args(arguments)
        .output()
        .map_err(|_| CommandError::StartFailed)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CommandError::Rejected)
    }
}
