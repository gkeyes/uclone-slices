use crate::domain::{AppIdentity, DataInodes, PackageEnabledState, PackageName};

use super::BRIDGE_MAIN_CLASS;

pub(super) fn launch_args(
    package: &PackageName,
    identity: &AppIdentity,
    base_inodes: DataInodes,
) -> Vec<String> {
    contract_args("launch-package", package, identity, base_inodes)
}

pub(super) fn restore_enabled_args(
    package: &PackageName,
    state: PackageEnabledState,
    identity: &AppIdentity,
    base_inodes: DataInodes,
) -> Vec<String> {
    let mut args = contract_args("restore-enabled", package, identity, base_inodes);
    args.push(super::enabled_state_arg(state).into());
    args
}

pub(super) fn restore_suspended_args(
    package: &PackageName,
    suspended: bool,
    identity: &AppIdentity,
    base_inodes: DataInodes,
) -> Vec<String> {
    let mut args = contract_args("restore-suspended", package, identity, base_inodes);
    args.push(if suspended { "true" } else { "false" }.into());
    args
}

fn contract_args(
    operation: &str,
    package: &PackageName,
    identity: &AppIdentity,
    base_inodes: DataInodes,
) -> Vec<String> {
    vec![
        "/system/bin".into(),
        BRIDGE_MAIN_CLASS.into(),
        operation.into(),
        package.as_str().into(),
        identity.uid().to_string(),
        identity.signature_sha256().into(),
        identity.version_code().to_string(),
        identity.code_path().into(),
        base_inodes.ce().get().to_string(),
        base_inodes.de().get().to_string(),
    ]
}
