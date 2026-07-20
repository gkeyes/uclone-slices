#![allow(
    unreachable_pub,
    dead_code,
    unused_imports,
    reason = "path-included integration fixtures call fixed command constructors"
)]

use std::ffi::{OsStr, OsString};
use std::path::Path;

use crate::android::{AndroidCommand, CommandError, CommandKind, CommandRunner, DataDomain};
use crate::bridge::{
    ALLOWED_PACKAGE, ALLOWED_USER_ID, AppProcessRunner, BridgeClient, BridgeCommandRunner,
    BridgeErrorCode,
};
use crate::domain::{PackageEnabledState, PackageName, SlotId};
use crate::layout::RuntimeLayout;

use super::executor::{ProcessExecutor, ProcessFailure, ProcessInvocation, StdProcessExecutor};

const AM_PATH: &str = "/system/bin/am";
const MOUNT_PATH: &str = "/system/bin/mount";
const UMOUNT_PATH: &str = "/system/bin/umount";

/// Production fixed-command adapter for the allowlisted package and Android user zero.
#[derive(Debug)]
pub struct SystemCommandRunner<R = AppProcessRunner, E = StdProcessExecutor> {
    bridge: BridgeClient<R>,
    executor: E,
}

impl SystemCommandRunner<AppProcessRunner, StdProcessExecutor> {
    /// Constructs the production bridge and bounded process executor.
    pub const fn new() -> Self {
        Self::with_dependencies(AppProcessRunner::new(), StdProcessExecutor::new())
    }
}

impl Default for SystemCommandRunner<AppProcessRunner, StdProcessExecutor> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: BridgeCommandRunner, E> SystemCommandRunner<R, E> {
    /// Constructs an adapter from injected bridge and fixed-process boundaries.
    pub const fn with_dependencies(bridge_runner: R, executor: E) -> Self {
        Self {
            bridge: BridgeClient::new(bridge_runner),
            executor,
        }
    }

    /// Returns the injected process executor for deterministic inspection.
    pub const fn executor(&self) -> &E {
        &self.executor
    }

    /// Returns the owned bridge runner and process executor.
    pub fn into_dependencies(self) -> (R, E) {
        (self.bridge.into_inner(), self.executor)
    }
}

impl<R: BridgeCommandRunner, E: ProcessExecutor> CommandRunner for SystemCommandRunner<R, E> {
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError> {
        require_target(command)?;
        match command.kind() {
            CommandKind::DisableUser => {
                require_no_paths(command)?;
                self.set_enabled(PackageEnabledState::DisabledUser)
            }
            CommandKind::RestoreEnabled(state) => {
                require_no_paths(command)?;
                self.set_enabled(state)
            }
            CommandKind::RestoreSuspended(suspended) => {
                require_no_paths(command)?;
                self.bridge
                    .set_suspended(ALLOWED_PACKAGE, ALLOWED_USER_ID, suspended)
                    .map_err(|error| map_bridge_error(&error))
            }
            CommandKind::ForceStop => {
                require_no_paths(command)?;
                self.execute(&force_stop_invocation())
            }
            CommandKind::Bind(domain) | CommandKind::Unmount(domain) => {
                let invocation = mount_invocation(command, domain)?;
                self.execute(&invocation)
            }
        }
    }
}

impl<R: BridgeCommandRunner, E> SystemCommandRunner<R, E> {
    fn set_enabled(&mut self, state: PackageEnabledState) -> Result<(), CommandError> {
        self.bridge
            .set_enabled(ALLOWED_PACKAGE, ALLOWED_USER_ID, state)
            .map_err(|error| map_bridge_error(&error))
    }
}

impl<R, E: ProcessExecutor> SystemCommandRunner<R, E> {
    fn execute(&mut self, invocation: &ProcessInvocation) -> Result<(), CommandError> {
        let output = self
            .executor
            .execute(invocation)
            .map_err(|error| map_process_error(&error))?;
        if output.exit_code() == Some(0) {
            Ok(())
        } else {
            Err(CommandError::Rejected)
        }
    }
}

fn require_target(command: &AndroidCommand) -> Result<(), CommandError> {
    if command.package_name().as_str() == ALLOWED_PACKAGE {
        Ok(())
    } else {
        Err(CommandError::Rejected)
    }
}

fn require_no_paths(command: &AndroidCommand) -> Result<(), CommandError> {
    if command.source().is_none() && command.target().is_none() {
        Ok(())
    } else {
        Err(CommandError::Rejected)
    }
}

pub fn force_stop_invocation() -> ProcessInvocation {
    ProcessInvocation::fixed(
        AM_PATH,
        ["force-stop", "--user", "0", ALLOWED_PACKAGE].map(OsString::from),
    )
}

pub fn mount_invocation(
    command: &AndroidCommand,
    domain: DataDomain,
) -> Result<ProcessInvocation, CommandError> {
    let package = command.package_name();
    let canonical = RuntimeLayout::slot_paths(package, &SlotId::base());
    let expected_target = domain_path(domain, canonical.ce(), canonical.de());
    let target = command.target().ok_or(CommandError::Rejected)?;
    if target != expected_target {
        return Err(CommandError::Rejected);
    }
    match command.kind() {
        CommandKind::Bind(actual) if actual == domain => {
            let source = command.source().ok_or(CommandError::Rejected)?;
            bind_invocation_for_paths(package, domain, source, target)
        }
        CommandKind::Unmount(actual) if actual == domain && command.source().is_none() => {
            Ok(unmount_invocation_for_target(target))
        }
        _ => Err(CommandError::Rejected),
    }
}

pub fn bind_invocation_for_paths(
    package: &PackageName,
    domain: DataDomain,
    source: &Path,
    target: &Path,
) -> Result<ProcessInvocation, CommandError> {
    let canonical = RuntimeLayout::slot_paths(package, &SlotId::base());
    if target != domain_path(domain, canonical.ce(), canonical.de()) {
        return Err(CommandError::Rejected);
    }
    validate_slot_source(package, domain, source)?;
    Ok(ProcessInvocation::fixed(
        MOUNT_PATH,
        [
            OsString::from("--bind"),
            source.as_os_str().to_os_string(),
            target.as_os_str().to_os_string(),
        ],
    ))
}

pub fn unmount_invocation_for_target(target: &Path) -> ProcessInvocation {
    ProcessInvocation::fixed(UMOUNT_PATH, [target.as_os_str().to_os_string()])
}

fn validate_slot_source(
    package: &PackageName,
    domain: DataDomain,
    source: &Path,
) -> Result<(), CommandError> {
    let raw_slot = source
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or(CommandError::Rejected)?;
    let slot = SlotId::parse(raw_slot).map_err(|_| CommandError::Rejected)?;
    if slot.is_base() {
        return Err(CommandError::Rejected);
    }
    let expected = RuntimeLayout::slot_paths(package, &slot);
    let expected_source = domain_path(domain, expected.ce(), expected.de());
    if source == expected_source {
        Ok(())
    } else {
        Err(CommandError::Rejected)
    }
}

const fn domain_path<'a>(domain: DataDomain, ce: &'a Path, de: &'a Path) -> &'a Path {
    match domain {
        DataDomain::Ce => ce,
        DataDomain::De => de,
    }
}

fn map_bridge_error(error: &crate::bridge::BridgeError) -> CommandError {
    if error.code() == BridgeErrorCode::RunnerUnavailable {
        CommandError::StartFailed
    } else {
        CommandError::Rejected
    }
}

const fn map_process_error(error: &ProcessFailure) -> CommandError {
    match error {
        ProcessFailure::Io(_) => CommandError::StartFailed,
        ProcessFailure::TimedOut | ProcessFailure::OutputTooLarge { .. } => CommandError::Rejected,
    }
}
