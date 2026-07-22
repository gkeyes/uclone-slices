use super::WireRequest;
use crate::domain::{PackageName, SlotId};
use crate::protocol::{Command, ProtocolError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldRule {
    Forbidden,
    Required,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct WireSchema {
    package: FieldRule,
    slot: FieldRule,
    display_name: FieldRule,
    seed_mode: FieldRule,
    direct_boot: FieldRule,
}

impl WireSchema {
    pub(super) const NONE: Self = Self::new(
        FieldRule::Forbidden,
        FieldRule::Forbidden,
        FieldRule::Forbidden,
        FieldRule::Forbidden,
        FieldRule::Forbidden,
    );
    pub(super) const PACKAGE: Self = Self {
        package: FieldRule::Required,
        ..Self::NONE
    };
    pub(super) const PACKAGE_SLOT: Self = Self {
        package: FieldRule::Required,
        slot: FieldRule::Required,
        ..Self::NONE
    };
    pub(super) const ENROLL: Self = Self {
        package: FieldRule::Required,
        direct_boot: FieldRule::Optional,
        ..Self::NONE
    };
    pub(super) const CREATE: Self = Self {
        package: FieldRule::Required,
        display_name: FieldRule::Required,
        seed_mode: FieldRule::Required,
        ..Self::NONE
    };
    pub(super) const RENAME: Self = Self {
        package: FieldRule::Required,
        slot: FieldRule::Required,
        display_name: FieldRule::Required,
        ..Self::NONE
    };

    const fn new(
        package: FieldRule,
        slot: FieldRule,
        display_name: FieldRule,
        seed_mode: FieldRule,
        direct_boot: FieldRule,
    ) -> Self {
        Self {
            package,
            slot,
            display_name,
            seed_mode,
            direct_boot,
        }
    }
}

pub(super) fn validate(wire: &WireRequest, schema: WireSchema) -> Result<(), ProtocolError> {
    validate_field(wire.package.is_some(), schema.package, "package")?;
    validate_field(wire.slot.is_some(), schema.slot, "slot")?;
    validate_field(
        wire.display_name.is_some(),
        schema.display_name,
        "display_name",
    )?;
    validate_field(wire.seed_mode.is_some(), schema.seed_mode, "seed_mode")?;
    validate_field(
        wire.accept_direct_boot_conditional.is_some(),
        schema.direct_boot,
        "accept_direct_boot_conditional",
    )
}

const fn validate_field(
    present: bool,
    rule: FieldRule,
    name: &'static str,
) -> Result<(), ProtocolError> {
    match (present, rule) {
        (true, FieldRule::Forbidden) | (false, FieldRule::Required) => {
            Err(ProtocolError::UnexpectedField(name))
        }
        (true, FieldRule::Required | FieldRule::Optional)
        | (false, FieldRule::Forbidden | FieldRule::Optional) => Ok(()),
    }
}

pub(super) fn no_fields(wire: &WireRequest, command: Command) -> Result<Command, ProtocolError> {
    validate(wire, WireSchema::NONE)?;
    Ok(command)
}

pub(super) fn package_only(
    wire: &WireRequest,
    constructor: fn(PackageName) -> Command,
) -> Result<Command, ProtocolError> {
    validate(wire, WireSchema::PACKAGE)?;
    Ok(constructor(required_package(wire)?))
}

pub(super) fn package_slot(
    wire: &WireRequest,
    constructor: fn(PackageName, SlotId) -> Command,
) -> Result<Command, ProtocolError> {
    validate(wire, WireSchema::PACKAGE_SLOT)?;
    Ok(constructor(
        required_package(wire)?,
        wire.slot.clone().ok_or(ProtocolError::MissingSlot)?,
    ))
}

pub(super) fn required_package(wire: &WireRequest) -> Result<PackageName, ProtocolError> {
    wire.package.clone().ok_or(ProtocolError::MissingPackage)
}
