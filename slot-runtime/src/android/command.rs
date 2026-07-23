use std::path::{Path, PathBuf};

use crate::domain::{ManagedPackage, PackageEnabledState, PackageName};

#[doc = "One Android app-data encryption domain."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDomain {
    #[doc = "Credential-encrypted app data."]
    Ce,
    #[doc = "Device-encrypted app data."]
    De,
}

#[doc = "Typed operation represented by one fixed Android process invocation."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKind {
    #[doc = "Disable the allowlisted package for user zero."]
    DisableUser,
    #[doc = "Force-stop the allowlisted package for user zero."]
    ForceStop,
    #[doc = "Open the system-resolved launcher entry for the validated package."]
    LaunchPackage,
    #[doc = "Restore the captured per-user enabled setting."]
    RestoreEnabled(PackageEnabledState),
    #[doc = "Restore the captured suspension setting."]
    RestoreSuspended(bool),
    #[doc = "Bind one derived slot directory over its canonical directory."]
    Bind(DataDomain),
    #[doc = "Unmount one canonical slot directory."]
    Unmount(DataDomain),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CommandSpec {
    Package {
        kind: CommandKind,
        package: PackageName,
    },
    Launch {
        expected: ManagedPackage,
    },
    ContractMutation {
        kind: CommandKind,
        expected: ManagedPackage,
    },
    Bind {
        domain: DataDomain,
        package: PackageName,
        source: PathBuf,
        target: PathBuf,
    },
    Unmount {
        domain: DataDomain,
        package: PackageName,
        target: PathBuf,
    },
}

#[doc = "Opaque typed command whose filesystem paths can only be built by the adapter."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidCommand(pub(super) CommandSpec);

impl AndroidCommand {
    pub(super) fn package(kind: CommandKind, package: &PackageName) -> Self {
        Self(CommandSpec::Package {
            kind,
            package: package.clone(),
        })
    }

    pub(super) fn launch(package: &ManagedPackage) -> Self {
        Self(CommandSpec::Launch {
            expected: package.clone(),
        })
    }

    pub(super) fn restore_enabled(package: &ManagedPackage, state: PackageEnabledState) -> Self {
        Self(CommandSpec::ContractMutation {
            kind: CommandKind::RestoreEnabled(state),
            expected: package.clone(),
        })
    }

    pub(super) fn restore_suspended(package: &ManagedPackage, suspended: bool) -> Self {
        Self(CommandSpec::ContractMutation {
            kind: CommandKind::RestoreSuspended(suspended),
            expected: package.clone(),
        })
    }

    pub(super) fn bind_ce(package: &PackageName, source: &Path, target: &Path) -> Self {
        Self(CommandSpec::Bind {
            domain: DataDomain::Ce,
            package: package.clone(),
            source: source.to_path_buf(),
            target: target.to_path_buf(),
        })
    }

    pub(super) fn bind_de(package: &PackageName, source: &Path, target: &Path) -> Self {
        Self(CommandSpec::Bind {
            domain: DataDomain::De,
            package: package.clone(),
            source: source.to_path_buf(),
            target: target.to_path_buf(),
        })
    }

    pub(super) fn unmount(domain: DataDomain, package: &PackageName, target: &Path) -> Self {
        Self(CommandSpec::Unmount {
            domain,
            package: package.clone(),
            target: target.to_path_buf(),
        })
    }

    #[doc = "Returns the typed operation without exposing a shell command string."]
    pub const fn kind(&self) -> CommandKind {
        match &self.0 {
            CommandSpec::Package { kind, .. } => *kind,
            CommandSpec::Launch { .. } => CommandKind::LaunchPackage,
            CommandSpec::ContractMutation { kind, .. } => *kind,
            CommandSpec::Bind { domain, .. } => CommandKind::Bind(*domain),
            CommandSpec::Unmount { domain, .. } => CommandKind::Unmount(*domain),
        }
    }

    #[doc = "Returns the validated package argument."]
    pub const fn package_name(&self) -> &PackageName {
        match &self.0 {
            CommandSpec::Package { package, .. }
            | CommandSpec::Bind { package, .. }
            | CommandSpec::Unmount { package, .. } => package,
            CommandSpec::Launch { expected } | CommandSpec::ContractMutation { expected, .. } => {
                expected.package_name()
            }
        }
    }

    #[doc = "Returns the complete enrolled contract required for release or launch."]
    pub const fn expected_package(&self) -> Option<&ManagedPackage> {
        match &self.0 {
            CommandSpec::Launch { expected } | CommandSpec::ContractMutation { expected, .. } => {
                Some(expected)
            }
            CommandSpec::Package { .. }
            | CommandSpec::Bind { .. }
            | CommandSpec::Unmount { .. } => None,
        }
    }

    #[doc = "Returns the derived bind source, if this is a bind operation."]
    pub fn source(&self) -> Option<&Path> {
        match &self.0 {
            CommandSpec::Bind { source, .. } => Some(source),
            CommandSpec::Package { .. }
            | CommandSpec::Launch { .. }
            | CommandSpec::ContractMutation { .. }
            | CommandSpec::Unmount { .. } => None,
        }
    }

    #[doc = "Returns the derived mount target, if this is a mount operation."]
    pub fn target(&self) -> Option<&Path> {
        match &self.0 {
            CommandSpec::Bind { target, .. } | CommandSpec::Unmount { target, .. } => Some(target),
            CommandSpec::Package { .. }
            | CommandSpec::Launch { .. }
            | CommandSpec::ContractMutation { .. } => None,
        }
    }
}

#[doc = "Failure to start or successfully complete one fixed Android command."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[doc = "The fixed executable could not be started."]
    #[error("fixed Android executable could not be started")]
    StartFailed,
    #[doc = "The fixed executable returned a non-success status."]
    #[error("fixed Android command was rejected")]
    Rejected,
    #[doc = "The validated package has no enabled launcher entry for user zero."]
    #[error("validated package has no launcher entry")]
    LaunchEntryNotFound,
    #[doc = "PackageManager identity changed before the launcher activity could start."]
    #[error("validated package identity changed before launch")]
    IdentityChanged,
    #[doc = "Package metadata, Base anchors, or install-session state changed."]
    #[error("validated package state changed before mutation")]
    PackageStateChanged,
}

#[doc = "Injected execution boundary for typed Android commands."]
pub trait CommandRunner: core::fmt::Debug {
    #[doc = "Runs exactly one typed command without a shell."]
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError>;
}
