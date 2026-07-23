use std::io::{self, Write};
use std::os::unix::net::UnixStream;

use crate::layout::RuntimeLayout;
use crate::lifecycle::LifecycleState;

use super::{CliError, fail};
use wire::{CompatPayload, CompatResponse, exchange, request};

const MAX_MANAGED_APPS: usize = 64;
const V1_PROBE_ID: &str = "upgrade-probe-v1";
const V2_PROBE_ID: &str = "upgrade-probe-v2";
const APPS_ID: &str = "upgrade-apps";

mod wire;

/// A fixed upgrade-readiness check failed without changing Runtime state.
#[derive(Debug, thiserror::Error)]
pub enum UpgradeReadinessError {
    /// The fixed Runtime socket could not complete a bounded exchange.
    #[error("fixed Runtime transport failed")]
    Transport(#[source] io::Error),
    /// A response was malformed or did not match the fixed request.
    #[error("installed Runtime returned an invalid compatibility frame")]
    InvalidFrame,
    /// The installed Runtime uses neither supported upgrade protocol.
    #[error("installed Runtime protocol schema is unsupported")]
    UnsupportedSchema,
    /// The installed Runtime rejected a read-only compatibility request.
    #[error("installed Runtime rejected the read-only compatibility request")]
    Rejected,
    /// The installed Runtime reported an unsafe build identity.
    #[error("installed Runtime build identity is invalid")]
    InvalidBuildIdentity,
    /// At least one durable package is not committed to native Base.
    #[error("a managed package is not on native Base")]
    NonBase,
    /// At least one durable package is not in normal lifecycle state.
    #[error("a managed package lifecycle is not normal")]
    NonNormal,
    /// The bounded managed-package report exceeded its supported size.
    #[error("managed package count exceeds the upgrade bound")]
    TooManyApps,
}

pub(super) fn run<O: Write, E: Write>(output: &mut O, diagnostics: &mut E) -> Result<(), CliError> {
    let count = match check_with(|| UnixStream::connect(RuntimeLayout::socket())) {
        Ok(count) => count,
        Err(error) => return fail(CliError::UpgradeReadiness(error), diagnostics),
    };
    writeln!(output, "{count}").map_err(CliError::Io)?;
    output.flush().map_err(CliError::Io)
}

fn check_with<C>(mut connect: C) -> Result<usize, UpgradeReadinessError>
where
    C: FnMut() -> io::Result<UnixStream>,
{
    let v1_request = request(1, V1_PROBE_ID, "probe", None)?;
    let v1_probe = exchange(&mut connect, &v1_request)?;
    match v1_probe.schema_version {
        1 => {
            validate_probe(&v1_probe, 1, V1_PROBE_ID)?;
            check_apps(&mut connect, 1, None)
        }
        2 => {
            validate_schema_rejection(&v1_probe, V1_PROBE_ID)?;
            let v2_request = request(2, V2_PROBE_ID, "probe", None)?;
            let v2_probe = exchange(&mut connect, &v2_request)?;
            validate_probe(&v2_probe, 2, V2_PROBE_ID)?;
            let build_id = probe_build_id(&v2_probe)?;
            check_apps(&mut connect, 2, Some(build_id.as_str()))
        }
        _ => Err(UpgradeReadinessError::UnsupportedSchema),
    }
}

fn check_apps<C>(
    connect: &mut C,
    schema: u32,
    build_id: Option<&str>,
) -> Result<usize, UpgradeReadinessError>
where
    C: FnMut() -> io::Result<UnixStream>,
{
    let apps_request = request(schema, APPS_ID, "list_managed_apps", build_id)?;
    let response = exchange(connect, &apps_request)?;
    if response.schema_version != schema || response.request_id != APPS_ID {
        return Err(UpgradeReadinessError::InvalidFrame);
    }
    let apps = success_payload(response)?;
    let CompatPayload::ManagedApps(report) = apps else {
        return Err(UpgradeReadinessError::InvalidFrame);
    };
    if report.apps.len() > MAX_MANAGED_APPS {
        return Err(UpgradeReadinessError::TooManyApps);
    }
    for app in &report.apps {
        if !app.active_slot.is_base() {
            return Err(UpgradeReadinessError::NonBase);
        }
        if app.lifecycle != LifecycleState::Normal {
            return Err(UpgradeReadinessError::NonNormal);
        }
        let _ = app.package.as_str();
    }
    Ok(report.apps.len())
}

fn validate_probe(
    response: &CompatResponse,
    schema: u32,
    request_id: &str,
) -> Result<(), UpgradeReadinessError> {
    if response.schema_version != schema || response.request_id != request_id {
        return Err(UpgradeReadinessError::InvalidFrame);
    }
    let CompatPayload::ProbeReport(probe) = success_payload_ref(response)? else {
        return Err(UpgradeReadinessError::InvalidFrame);
    };
    if probe.ready && probe.user_unlocked && probe.ce_de_supported && !probe.recovery_only {
        Ok(())
    } else {
        Err(UpgradeReadinessError::Rejected)
    }
}

fn validate_schema_rejection(
    response: &CompatResponse,
    request_id: &str,
) -> Result<(), UpgradeReadinessError> {
    let accepted_error = matches!(
        response.error_code.as_deref(),
        Some("invalid_request" | "unsupported_schema")
    );
    if response.request_id == request_id
        && response.status == "error"
        && accepted_error
        && response.payload.is_none()
    {
        Ok(())
    } else {
        Err(UpgradeReadinessError::InvalidFrame)
    }
}

fn probe_build_id(response: &CompatResponse) -> Result<String, UpgradeReadinessError> {
    let CompatPayload::ProbeReport(probe) = success_payload_ref(response)? else {
        return Err(UpgradeReadinessError::InvalidFrame);
    };
    let build_id = probe
        .build_id
        .as_deref()
        .ok_or(UpgradeReadinessError::InvalidBuildIdentity)?;
    let version_present = probe
        .runtime_version
        .as_deref()
        .is_some_and(|value| !value.is_empty() && value.len() <= 128);
    if version_present && safe_identity(build_id) {
        Ok(build_id.to_owned())
    } else {
        Err(UpgradeReadinessError::InvalidBuildIdentity)
    }
}

fn success_payload(response: CompatResponse) -> Result<CompatPayload, UpgradeReadinessError> {
    if response.status != "ok" || response.error_code.is_some() {
        return Err(UpgradeReadinessError::Rejected);
    }
    response.payload.ok_or(UpgradeReadinessError::InvalidFrame)
}

fn success_payload_ref(response: &CompatResponse) -> Result<&CompatPayload, UpgradeReadinessError> {
    if response.status != "ok" || response.error_code.is_some() {
        return Err(UpgradeReadinessError::Rejected);
    }
    response
        .payload
        .as_ref()
        .ok_or(UpgradeReadinessError::InvalidFrame)
}

fn safe_identity(value: &str) -> bool {
    (1..=128).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

#[cfg(test)]
mod tests;
