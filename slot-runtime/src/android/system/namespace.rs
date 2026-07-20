#![allow(
    unreachable_pub,
    dead_code,
    unused_imports,
    reason = "path-included integration fixtures inspect namespace argv"
)]

use std::ffi::OsString;
use std::path::Path;

use crate::android::ProbeError;
use crate::domain::{DataInodes, PackageName, SlotId};
use crate::layout::RuntimeLayout;

use super::executor::{ProcessExecutor, ProcessFailure, ProcessInvocation};
use super::facts::ProcessSet;
use super::parse::parse_inode_pair;

const NSENTER_PATH: &str = "/system/bin/nsenter";
const STAT_PATH: &str = "/system/bin/stat";

pub(super) fn namespace_consensus<E: ProcessExecutor>(
    executor: &mut E,
    package: &PackageName,
    processes: &ProcessSet,
) -> Result<DataInodes, ProbeError> {
    let base = RuntimeLayout::slot_paths(package, &SlotId::base());
    let mut agreed = None;
    for pid in processes.pids() {
        let invocation = namespace_stat_invocation(*pid, base.ce(), base.de());
        let output = executor
            .execute(&invocation)
            .map_err(|error| map_process_error(&error))?;
        if output.exit_code() != Some(0) {
            return Err(ProbeError::InvalidResponse);
        }
        let inodes = parse_inode_pair(output.stdout()).map_err(|_| ProbeError::InvalidResponse)?;
        if agreed.is_some_and(|sample| sample != inodes) {
            return Err(ProbeError::InvalidResponse);
        }
        agreed = Some(inodes);
    }
    agreed.ok_or(ProbeError::InvalidResponse)
}

pub fn namespace_stat_invocation(pid: u32, ce: &Path, de: &Path) -> ProcessInvocation {
    ProcessInvocation::fixed(
        NSENTER_PATH,
        [
            OsString::from("-t"),
            OsString::from(pid.to_string()),
            OsString::from("-m"),
            OsString::from("--"),
            OsString::from(STAT_PATH),
            OsString::from("-L"),
            OsString::from("-c"),
            OsString::from("%i"),
            OsString::from("--"),
            ce.as_os_str().to_os_string(),
            de.as_os_str().to_os_string(),
        ],
    )
}

const fn map_process_error(error: &ProcessFailure) -> ProbeError {
    match error {
        ProcessFailure::Io(_) | ProcessFailure::TimedOut => ProbeError::Unavailable,
        ProcessFailure::OutputTooLarge { .. } => ProbeError::InvalidResponse,
    }
}
