#![doc = "Fail-closed, fixed-command Android app_process bridge for package facts."]
#![allow(clippy::doc_markdown)]

mod client;
mod client_validation;
mod device;
mod error;
mod launch_command;
mod payload;
mod protocol;
mod response;
mod runner;
mod validation;

pub use client::BridgeClient;
pub use device::DeviceSnapshot;
pub use error::{BridgeError, BridgeErrorCode};
pub use payload::{AckPayload, BridgeGateSnapshot, BridgePayload};
pub use protocol::PackageSnapshot;
pub use response::{BridgeResponse, decode_response};
pub use runner::{AppProcessRunner, BridgeCommandRunner, BridgeRunnerError};

use error::invalid_response;

pub use crate::domain::PackageEnabledState;
use crate::domain::PackageName;
use crate::domain::{AppIdentity, DataInodes};

/// Current bridge response schema.
pub const BRIDGE_SCHEMA_VERSION: u32 = 1;
/// Maximum response size accepted from the app_process child, including no
/// implicit framing bytes.
pub const MAX_OUTPUT_BYTES: usize = 16 * 1024;
/// The only package and Android user addressable by this preview bridge.
pub const ALLOWED_PACKAGE: &str = crate::target::PACKAGE;
/// The only Android user addressable by this preview bridge.
pub const ALLOWED_USER_ID: u32 = crate::target::USER_ID;
/// Fixed app_process executable. This is deliberately not configurable.
pub const APP_PROCESS_PATH: &str = "/system/bin/app_process";
/// Fixed bridge APK classpath. This is deliberately not configurable.
pub const BRIDGE_CLASSPATH: &str = crate::target::BRIDGE_CLASSPATH;
/// Fixed Java entry point loaded by app_process.
pub const BRIDGE_MAIN_CLASS: &str = "com.uclone.slotbridge.Main";

/// The fixed bridge operation. It carries no caller-controlled strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeCommand {
    /// Read whether user 0's credential-encrypted device state is unlocked.
    DeviceStatus,
    /// Read a validated package's user 0 PackageManager state.
    PackageStatus(PackageName),
    /// Read only a validated package's enabled and suspended state.
    GateStatus(PackageName),
    /// Open only the enrolled identity's system-resolved launcher activity.
    LaunchPackage {
        /// Validated package identifier.
        package: PackageName,
        /// Enrolled UID, signing digest, and version checked immediately before start.
        expected_identity: AppIdentity,
        /// Immutable native Base CE/DE inode anchors checked immediately before start.
        expected_base_inodes: DataInodes,
    },
    /// Set a validated package's enabled-state enum for user 0.
    SetEnabled(PackageName, PackageEnabledState),
    /// Restore enabled state only while the complete enrolled contract still matches.
    RestoreEnabled {
        /// Validated package identifier.
        package: PackageName,
        /// Exact enabled state captured before gate acquisition.
        state: PackageEnabledState,
        /// Enrolled UID, signing digest, version, and code path.
        expected_identity: AppIdentity,
        /// Immutable native Base CE/DE inode anchors.
        expected_base_inodes: DataInodes,
    },
    /// Restore suspension only while the complete enrolled contract still matches.
    RestoreSuspended {
        /// Validated package identifier.
        package: PackageName,
        /// Exact suspension state captured before gate acquisition.
        suspended: bool,
        /// Enrolled UID, signing digest, version, and code path.
        expected_identity: AppIdentity,
        /// Immutable native Base CE/DE inode anchors.
        expected_base_inodes: DataInodes,
    },
}

impl BridgeCommand {
    /// Returns the exact argv passed after the fixed executable path.
    pub fn argv(&self) -> Vec<String> {
        match self {
            Self::DeviceStatus => fixed_args("probe-device"),
            Self::PackageStatus(package) => package_args("probe-package", package),
            Self::GateStatus(package) => package_args("probe-gate", package),
            Self::LaunchPackage {
                package,
                expected_identity,
                expected_base_inodes,
            } => launch_command::launch_args(package, expected_identity, *expected_base_inodes),
            Self::SetEnabled(package, state) => vec![
                "/system/bin".into(),
                BRIDGE_MAIN_CLASS.into(),
                "set-enabled".into(),
                package.as_str().into(),
                enabled_state_arg(*state).into(),
            ],
            Self::RestoreEnabled {
                package,
                state,
                expected_identity,
                expected_base_inodes,
            } => launch_command::restore_enabled_args(
                package,
                *state,
                expected_identity,
                *expected_base_inodes,
            ),
            Self::RestoreSuspended {
                package,
                suspended,
                expected_identity,
                expected_base_inodes,
            } => launch_command::restore_suspended_args(
                package,
                *suspended,
                expected_identity,
                *expected_base_inodes,
            ),
        }
    }

    pub(crate) fn session_request(&self) -> Vec<u8> {
        let mut request = self
            .argv()
            .into_iter()
            .skip(2)
            .collect::<Vec<_>>()
            .join("\t")
            .into_bytes();
        request.push(b'\n');
        request
    }

    /// Returns the fixed request id echoed by the Java bridge.
    pub const fn request_id(&self) -> &'static str {
        match self {
            Self::DeviceStatus => "device",
            Self::PackageStatus(_) => "package",
            Self::GateStatus(_) => "gate",
            Self::LaunchPackage { .. } => "launch-package",
            Self::SetEnabled(_, _) => "set-enabled",
            Self::RestoreEnabled { .. } => "restore-enabled",
            Self::RestoreSuspended { .. } => "restore-suspended",
        }
    }

    /// Returns the expected tagged payload name for this operation.
    pub const fn payload_name(&self) -> &'static str {
        match self {
            Self::DeviceStatus => "device",
            Self::PackageStatus(_) => "package",
            Self::GateStatus(_) => "gate",
            Self::LaunchPackage { .. }
            | Self::SetEnabled(_, _)
            | Self::RestoreEnabled { .. }
            | Self::RestoreSuspended { .. } => "ack",
        }
    }
}

fn fixed_args(operation: &str) -> Vec<String> {
    vec![
        "/system/bin".into(),
        BRIDGE_MAIN_CLASS.into(),
        operation.into(),
    ]
}

fn package_args(operation: &str, package: &PackageName) -> Vec<String> {
    vec![
        "/system/bin".into(),
        BRIDGE_MAIN_CLASS.into(),
        operation.into(),
        package.as_str().into(),
    ]
}

pub(super) const fn enabled_state_arg(state: PackageEnabledState) -> &'static str {
    match state {
        PackageEnabledState::Default => "default",
        PackageEnabledState::Enabled => "enabled",
        PackageEnabledState::Disabled => "disabled",
        PackageEnabledState::DisabledUser => "disabled_user",
        PackageEnabledState::DisabledUntilUsed => "disabled_until_used",
    }
}
