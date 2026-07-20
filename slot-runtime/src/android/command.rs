use std::path::{Path, PathBuf};

use crate::domain::{PackageEnabledState, PackageName};

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
        }
    }

    #[doc = "Returns the derived bind source, if this is a bind operation."]
    pub fn source(&self) -> Option<&Path> {
        match &self.0 {
            CommandSpec::Bind { source, .. } => Some(source),
            CommandSpec::Package { .. } | CommandSpec::Unmount { .. } => None,
        }
    }

    #[doc = "Returns the derived mount target, if this is a mount operation."]
    pub fn target(&self) -> Option<&Path> {
        match &self.0 {
            CommandSpec::Bind { target, .. } | CommandSpec::Unmount { target, .. } => Some(target),
            CommandSpec::Package { .. } => None,
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
}

#[doc = "Injected execution boundary for typed Android commands."]
pub trait CommandRunner: core::fmt::Debug {
    #[doc = "Runs exactly one typed command without a shell."]
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError>;
}
