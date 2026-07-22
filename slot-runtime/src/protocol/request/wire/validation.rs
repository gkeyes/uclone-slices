use super::WireRequest;
use crate::domain::{PackageName, SlotId};
use crate::protocol::{Command, ProtocolError};

pub(super) fn enroll_fields(wire: &WireRequest) -> Result<(), ProtocolError> {
    if wire.package.is_none() {
        return Err(ProtocolError::UnexpectedField("package"));
    }
    if wire.slot.is_some() {
        return Err(ProtocolError::UnexpectedField("slot"));
    }
    if wire.display_name.is_some() {
        return Err(ProtocolError::UnexpectedField("display_name"));
    }
    if wire.seed_mode.is_some() {
        return Err(ProtocolError::UnexpectedField("seed_mode"));
    }
    Ok(())
}

pub(super) fn no_fields(wire: &WireRequest, command: Command) -> Result<Command, ProtocolError> {
    reject(wire, false, false, false, false, false)?;
    Ok(command)
}

pub(super) fn package_only(
    wire: &WireRequest,
    constructor: fn(PackageName) -> Command,
) -> Result<Command, ProtocolError> {
    reject(wire, true, false, false, false, false)?;
    Ok(constructor(required_package(wire)?))
}

pub(super) fn package_slot(
    wire: &WireRequest,
    constructor: fn(PackageName, SlotId) -> Command,
) -> Result<Command, ProtocolError> {
    reject(wire, true, true, false, false, false)?;
    Ok(constructor(
        required_package(wire)?,
        wire.slot.clone().ok_or(ProtocolError::MissingSlot)?,
    ))
}

pub(super) fn required_package(wire: &WireRequest) -> Result<PackageName, ProtocolError> {
    wire.package.clone().ok_or(ProtocolError::MissingPackage)
}

#[allow(
    clippy::fn_params_excessive_bools,
    reason = "five wire fields are validated independently against a fixed command schema"
)]
pub(super) const fn reject(
    wire: &WireRequest,
    package: bool,
    slot: bool,
    display_name: bool,
    seed_mode: bool,
    direct_boot: bool,
) -> Result<(), ProtocolError> {
    if wire.package.is_some() != package {
        return Err(ProtocolError::UnexpectedField("package"));
    }
    if wire.slot.is_some() != slot {
        return Err(ProtocolError::UnexpectedField("slot"));
    }
    if wire.display_name.is_some() != display_name {
        return Err(ProtocolError::UnexpectedField("display_name"));
    }
    if wire.seed_mode.is_some() != seed_mode {
        return Err(ProtocolError::UnexpectedField("seed_mode"));
    }
    if wire.accept_direct_boot_conditional.is_some() != direct_boot {
        return Err(ProtocolError::UnexpectedField(
            "accept_direct_boot_conditional",
        ));
    }
    Ok(())
}
