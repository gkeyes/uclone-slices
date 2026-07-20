#![allow(
    unreachable_pub,
    dead_code,
    unused_imports,
    reason = "path-included integration fixtures exercise procfs readers"
)]

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use crate::domain::PackageName;

use super::facts::{FactError, MAX_PROCESS_COUNT, ProcessSet};
use super::{io, parse};

const PROC_ROOT: &str = "/proc";

pub fn mount_namespace_ids() -> Result<(u64, u64), FactError> {
    let daemon_link =
        fs::read_link(Path::new("/proc/self/ns/mnt")).map_err(|_| FactError::Unavailable)?;
    let init_link =
        fs::read_link(Path::new("/proc/1/ns/mnt")).map_err(|_| FactError::Unavailable)?;
    Ok((
        parse::parse_mount_namespace(daemon_link.as_os_str())?,
        parse::parse_mount_namespace(init_link.as_os_str())?,
    ))
}

pub fn package_processes(package: &PackageName, uid: u32) -> Result<ProcessSet, FactError> {
    package_processes_at(Path::new(PROC_ROOT), package, uid)
}

pub fn arm64_zygote_processes() -> Result<ProcessSet, FactError> {
    arm64_zygote_processes_at(Path::new(PROC_ROOT))
}

fn package_processes_at(
    proc_root: &Path,
    package: &PackageName,
    uid: u32,
) -> Result<ProcessSet, FactError> {
    let mut selected = Vec::new();
    for process in process_directories(proc_root)? {
        if process.uid != uid {
            continue;
        }
        let Some(cmdline) = read_process_cmdline(&process.path)? else {
            continue;
        };
        let name = parse::parse_process_name(&cmdline)?;
        if !is_package_process_name(name, package.as_str()) {
            return Err(FactError::Invalid);
        }
        selected.push(process.pid);
    }
    ProcessSet::new(selected)
}

pub(super) fn arm64_zygote_processes_at(proc_root: &Path) -> Result<ProcessSet, FactError> {
    let mut selected = Vec::new();
    for process in process_directories(proc_root)? {
        let Some(cmdline) = read_process_cmdline(&process.path)? else {
            continue;
        };
        if !first_field_is_zygote64(&cmdline) {
            continue;
        }
        let name = parse::parse_process_name(&cmdline)?;
        if name == "zygote64" {
            selected.push(process.pid);
        }
    }
    let processes = ProcessSet::new(selected)?;
    if processes.pids().is_empty() {
        Err(FactError::Invalid)
    } else {
        Ok(processes)
    }
}

#[derive(Debug)]
struct ProcessDirectory {
    pid: u32,
    uid: u32,
    path: PathBuf,
}

fn process_directories(proc_root: &Path) -> Result<Vec<ProcessDirectory>, FactError> {
    io::directory_inode(proc_root)?;
    let entries = fs::read_dir(proc_root).map_err(|_| FactError::Unavailable)?;
    let mut seen = BTreeSet::new();
    let mut processes = Vec::new();
    for (entry_index, entry) in entries.enumerate() {
        if entry_index >= MAX_PROCESS_COUNT {
            return Err(FactError::Invalid);
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(_) => return Err(FactError::Unavailable),
        };
        let Some(pid) = parse_pid(entry.file_name().as_os_str())? else {
            continue;
        };
        if !seen.insert(pid) {
            return Err(FactError::Invalid);
        }
        let path = entry.path();
        let Some(metadata) = io::optional_directory_metadata(&path)? else {
            continue;
        };
        processes.push(ProcessDirectory {
            pid,
            uid: metadata.uid(),
            path,
        });
    }
    Ok(processes)
}

fn parse_pid(name: &OsStr) -> Result<Option<u32>, FactError> {
    let bytes = name.as_bytes();
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Ok(None);
    }
    let raw = std::str::from_utf8(bytes).map_err(|_| FactError::Invalid)?;
    let pid = raw.parse::<u32>().map_err(|_| FactError::Invalid)?;
    if pid == 0 {
        Err(FactError::Invalid)
    } else {
        Ok(Some(pid))
    }
}

fn read_process_cmdline(process: &Path) -> Result<Option<Vec<u8>>, FactError> {
    io::read_bounded_if_exists(&process.join("cmdline"), io::MAX_CMDLINE_BYTES)
}

fn is_package_process_name(name: &str, package: &str) -> bool {
    name == package
        || name
            .strip_prefix(package)
            .and_then(|suffix| suffix.strip_prefix(':'))
            .is_some_and(|suffix| !suffix.is_empty())
}

fn first_field_is_zygote64(cmdline: &[u8]) -> bool {
    let field = cmdline.split(|byte| *byte == 0).next().unwrap_or_default();
    field == b"zygote64"
}
