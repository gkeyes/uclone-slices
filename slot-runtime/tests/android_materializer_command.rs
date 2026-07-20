#![doc = "Fixed Android materializer command construction tests."]
#![allow(
    dead_code,
    missing_docs,
    unreachable_pub,
    clippy::redundant_pub_crate,
    reason = "production command model is included selectively"
)]

use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Duration;

mod materializer {
    pub(crate) use uclone_slot_runtime::materializer::DataDomain;
}

mod target {
    pub(crate) use uclone_slot_runtime::target::FSPROBE_PATH;
}

#[path = "../src/android/materializer/command.rs"]
mod command;

use command::{MaterializerCommand, MaterializerCommandKind};
use materializer::DataDomain;

fn arguments(command: &MaterializerCommand) -> Vec<&OsStr> {
    command
        .arguments()
        .iter()
        .map(OsString::as_os_str)
        .collect()
}

#[test]
fn copy_uses_a_fixed_executable_and_literal_argv_without_a_shell() {
    let source = Path::new("/data/source;touch /data/escaped");
    let target = Path::new("/data/target $(false)");

    let command = MaterializerCommand::copy(DataDomain::Ce, source, target);

    assert_eq!(
        command.kind(),
        MaterializerCommandKind::Copy(DataDomain::Ce)
    );
    assert_eq!(command.executable(), Path::new("/system/bin/cp"));
    assert_eq!(
        arguments(&command),
        [
            OsStr::new("-a"),
            OsStr::new("--"),
            OsStr::new("/data/source;touch /data/escaped/."),
            target.as_os_str(),
        ]
    );
}

#[test]
fn ownership_mode_and_probe_commands_have_exact_fixed_argv() {
    let target = Path::new("/data/staging");
    let chown = MaterializerCommand::chown(DataDomain::De, 10_123, 10_123, target);
    let chmod = MaterializerCommand::chmod(DataDomain::De, 0o751, target);
    let selinux = MaterializerCommand::get_selinux(DataDomain::De, target);
    let fscrypt = MaterializerCommand::fscrypt_policy(DataDomain::De, target);

    assert_eq!(chown.executable(), Path::new("/system/bin/chown"));
    assert_eq!(
        arguments(&chown),
        [
            OsStr::new("--"),
            OsStr::new("10123:10123"),
            target.as_os_str()
        ]
    );
    assert_eq!(chmod.executable(), Path::new("/system/bin/chmod"));
    assert_eq!(
        arguments(&chmod),
        [OsStr::new("--"), OsStr::new("0751"), target.as_os_str()]
    );
    assert_eq!(selinux.executable(), Path::new("/system/bin/getfattr"));
    assert_eq!(
        arguments(&selinux),
        [
            OsStr::new("--only-values"),
            OsStr::new("-n"),
            OsStr::new("security.selinux"),
            target.as_os_str(),
        ]
    );
    assert_eq!(
        fscrypt.executable(),
        Path::new("/data/adb/modules/uclone-slices-preview/runtime/slot-fsprobe")
    );
    assert_eq!(
        arguments(&fscrypt),
        [OsStr::new("policy"), target.as_os_str()]
    );
}

#[test]
fn leading_dash_selinux_context_is_after_end_of_options() {
    let target = Path::new("/data/staging");
    let command =
        MaterializerCommand::chcon(DataDomain::Ce, "-u:object_r:app_data_file:s0", target);

    assert_eq!(
        command.kind(),
        MaterializerCommandKind::Chcon(DataDomain::Ce)
    );
    assert_eq!(command.executable(), Path::new("/system/bin/chcon"));
    assert_eq!(
        arguments(&command),
        [
            OsStr::new("-R"),
            OsStr::new("--"),
            OsStr::new("-u:object_r:app_data_file:s0"),
            target.as_os_str(),
        ]
    );
}

#[test]
fn recursive_materializer_operations_get_a_long_but_bounded_deadline() {
    let target = Path::new("/data/staging");
    let copy = MaterializerCommand::copy(DataDomain::Ce, Path::new("/data/source"), target);
    let chcon = MaterializerCommand::chcon(DataDomain::De, "u:object_r:app_data_file:s0", target);
    let getfattr = MaterializerCommand::get_selinux(DataDomain::De, target);

    assert_eq!(copy.execution_timeout(), Duration::from_mins(30));
    assert_eq!(chcon.execution_timeout(), Duration::from_mins(30));
    assert_eq!(getfattr.execution_timeout(), Duration::from_secs(5));
}
