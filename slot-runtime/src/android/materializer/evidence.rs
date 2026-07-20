use std::path::Path;

use crate::catalog::PathSecurityProof;
use crate::materializer::{BackendFailure, DataDomain};

use super::command::MaterializerCommand;
use super::executor::{
    MAX_EXECUTOR_OUTPUT, MaterializerExecError, MaterializerExecutor, MaterializerOutput,
};
use super::tree::TreeInspection;

const MAX_SELINUX_CONTEXT: usize = 1_024;

pub(super) fn capture_security<E: MaterializerExecutor>(
    executor: &mut E,
    path: &Path,
    domain: DataDomain,
    tree: &TreeInspection,
) -> Result<PathSecurityProof, BackendFailure> {
    let context = read_selinux(executor, path, domain)?;
    let policy = read_fscrypt_policy(executor, path, domain)?;
    PathSecurityProof::new(tree.uid(), tree.gid(), tree.mode(), &context, &policy)
        .map_err(|_| BackendFailure::new("security_proof_invalid"))
}

pub(super) fn run_quiet<E: MaterializerExecutor>(
    executor: &mut E,
    command: &MaterializerCommand,
    failure_code: &str,
) -> Result<(), BackendFailure> {
    let output = executor
        .execute(command)
        .map_err(|_| BackendFailure::new(failure_code))?;
    ensure_bounded(&output)?;
    if output.stdout().is_empty() {
        Ok(())
    } else {
        Err(BackendFailure::new("command_unexpected_output"))
    }
}

pub(super) fn read_selinux<E: MaterializerExecutor>(
    executor: &mut E,
    path: &Path,
    domain: DataDomain,
) -> Result<String, BackendFailure> {
    let command = MaterializerCommand::get_selinux(domain, path);
    let output = executor
        .execute(&command)
        .map_err(|_| BackendFailure::new("selinux_probe_failed"))?;
    ensure_bounded(&output)?;
    parse_selinux(output.stdout())
}

pub(super) fn read_fscrypt_policy<E: MaterializerExecutor>(
    executor: &mut E,
    path: &Path,
    domain: DataDomain,
) -> Result<String, BackendFailure> {
    let command = MaterializerCommand::fscrypt_policy(domain, path);
    let output = executor.execute(&command).map_err(|error| match error {
        MaterializerExecError::StartFailed => BackendFailure::new("fscrypt_helper_unavailable"),
        MaterializerExecError::Rejected
        | MaterializerExecError::OutputTooLarge
        | MaterializerExecError::ReadFailed
        | MaterializerExecError::TimedOut => BackendFailure::new("fscrypt_helper_failed"),
    })?;
    ensure_bounded(&output)?;
    parse_policy(output.stdout())
}

pub(super) fn parse_selinux(bytes: &[u8]) -> Result<String, BackendFailure> {
    let value = trim_transport_suffix(bytes);
    if value.is_empty()
        || value.len() > MAX_SELINUX_CONTEXT
        || value
            .iter()
            .any(|byte| matches!(byte, b'\0' | b'\n' | b'\r'))
    {
        return Err(BackendFailure::new("selinux_context_invalid"));
    }
    std::str::from_utf8(value)
        .map(str::to_owned)
        .map_err(|_| BackendFailure::new("selinux_context_invalid"))
}

pub(super) fn parse_policy(bytes: &[u8]) -> Result<String, BackendFailure> {
    let value = trim_line_suffix(bytes);
    if value.len() != 64 || !value.iter().all(u8::is_ascii_hexdigit) {
        return Err(BackendFailure::new("fscrypt_policy_invalid"));
    }
    let text =
        std::str::from_utf8(value).map_err(|_| BackendFailure::new("fscrypt_policy_invalid"))?;
    Ok(text.to_ascii_lowercase())
}

fn ensure_bounded(output: &MaterializerOutput) -> Result<(), BackendFailure> {
    if output.stdout().len() <= MAX_EXECUTOR_OUTPUT {
        Ok(())
    } else {
        Err(BackendFailure::new("command_output_too_large"))
    }
}

fn trim_transport_suffix(mut value: &[u8]) -> &[u8] {
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b'\0' | b'\n' | b'\r'))
    {
        value = value.split_last().map_or(value, |(_, rest)| rest);
    }
    value
}

fn trim_line_suffix(mut value: &[u8]) -> &[u8] {
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b'\n' | b'\r'))
    {
        value = value.split_last().map_or(value, |(_, rest)| rest);
    }
    value
}
