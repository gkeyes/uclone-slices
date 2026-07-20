use serde::{Deserialize, Serialize, Serializer};

use super::{Command, Request, RequestId};
use crate::domain::{PackageName, SlotId};
use crate::protocol::{ProtocolError, SCHEMA_VERSION};
use crate::slot_metadata::{SlotDisplayName, SlotSeedMode};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireRequest {
    schema_version: u32,
    request_id: RequestId,
    command: String,
    #[serde(default)]
    package: Option<PackageName>,
    #[serde(default)]
    slot: Option<SlotId>,
    #[serde(default)]
    display_name: Option<SlotDisplayName>,
    #[serde(default)]
    seed_mode: Option<SlotSeedMode>,
}

#[derive(Serialize)]
struct WireRequestOut<'a> {
    schema_version: u32,
    request_id: &'a RequestId,
    command: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<&'a PackageName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<&'a SlotId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a SlotDisplayName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed_mode: Option<SlotSeedMode>,
}

pub(super) fn serialize<S: Serializer>(
    request_id: &RequestId,
    command: &Command,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let (name, package, slot, display_name, seed_mode) = fields(command);
    WireRequestOut {
        schema_version: SCHEMA_VERSION,
        request_id,
        command: name,
        package,
        slot,
        display_name,
        seed_mode,
    }
    .serialize(serializer)
}

type Fields<'a> = (
    &'static str,
    Option<&'a PackageName>,
    Option<&'a SlotId>,
    Option<&'a SlotDisplayName>,
    Option<SlotSeedMode>,
);

const fn fields(command: &Command) -> Fields<'_> {
    match command {
        Command::Probe => ("probe", None, None, None, None),
        Command::InspectPackage { package } => ("inspect_package", Some(package), None, None, None),
        Command::ListManagedApps => ("list_managed_apps", None, None, None, None),
        Command::EnrollPackage { package } => ("enroll_package", Some(package), None, None, None),
        Command::StatusPackage { package } => ("status_package", Some(package), None, None, None),
        Command::CreateSlot {
            package,
            display_name,
            seed_mode,
        } => (
            "create_slot",
            Some(package),
            None,
            Some(display_name),
            Some(*seed_mode),
        ),
        Command::ListSlots { package } => ("list_slots", Some(package), None, None, None),
        Command::Switch { package, slot } => ("switch", Some(package), Some(slot), None, None),
        Command::RenameSlot {
            package,
            slot,
            display_name,
        } => (
            "rename_slot",
            Some(package),
            Some(slot),
            Some(display_name),
            None,
        ),
        Command::DeleteSlot { package, slot } => {
            ("delete_slot", Some(package), Some(slot), None, None)
        }
        Command::Reconcile => ("reconcile", None, None, None, None),
        Command::ReconcilePackage { package } => {
            ("reconcile_package", Some(package), None, None, None)
        }
        Command::RetirePackage { package } => ("retire_package", Some(package), None, None, None),
        Command::RescueToBase { package } => ("rescue_to_base", Some(package), None, None, None),
    }
}

pub(super) fn from_wire(wire: WireRequest) -> Result<Request, ProtocolError> {
    if wire.schema_version != SCHEMA_VERSION {
        return Err(ProtocolError::UnsupportedSchema(wire.schema_version));
    }
    let command = match wire.command.as_str() {
        "probe" => no_fields(&wire, Command::Probe)?,
        "inspect_package" => package_only(&wire, |package| Command::InspectPackage { package })?,
        "list_managed_apps" => no_fields(&wire, Command::ListManagedApps)?,
        "enroll_package" => package_only(&wire, |package| Command::EnrollPackage { package })?,
        "status_package" => package_only(&wire, |package| Command::StatusPackage { package })?,
        "create_slot" => {
            reject(&wire, true, false, true, true)?;
            Command::CreateSlot {
                package: required_package(&wire)?,
                display_name: wire
                    .display_name
                    .clone()
                    .ok_or(ProtocolError::UnexpectedField("display_name"))?,
                seed_mode: wire
                    .seed_mode
                    .ok_or(ProtocolError::UnexpectedField("seed_mode"))?,
            }
        }
        "list_slots" => package_only(&wire, |package| Command::ListSlots { package })?,
        "switch" => package_slot(&wire, |package, slot| Command::Switch { package, slot })?,
        "rename_slot" => {
            reject(&wire, true, true, true, false)?;
            Command::RenameSlot {
                package: required_package(&wire)?,
                slot: wire.slot.clone().ok_or(ProtocolError::MissingSlot)?,
                display_name: wire
                    .display_name
                    .clone()
                    .ok_or(ProtocolError::UnexpectedField("display_name"))?,
            }
        }
        "delete_slot" => {
            package_slot(&wire, |package, slot| Command::DeleteSlot { package, slot })?
        }
        "reconcile" => no_fields(&wire, Command::Reconcile)?,
        "reconcile_package" => {
            package_only(&wire, |package| Command::ReconcilePackage { package })?
        }
        "retire_package" => package_only(&wire, |package| Command::RetirePackage { package })?,
        "rescue_to_base" => package_only(&wire, |package| Command::RescueToBase { package })?,
        unknown => return Err(ProtocolError::UnknownCommand(unknown.to_owned())),
    };
    Request::new(wire.request_id, command)
}

fn no_fields(wire: &WireRequest, command: Command) -> Result<Command, ProtocolError> {
    reject(wire, false, false, false, false)?;
    Ok(command)
}

fn package_only(
    wire: &WireRequest,
    constructor: fn(PackageName) -> Command,
) -> Result<Command, ProtocolError> {
    reject(wire, true, false, false, false)?;
    Ok(constructor(required_package(wire)?))
}

fn package_slot(
    wire: &WireRequest,
    constructor: fn(PackageName, SlotId) -> Command,
) -> Result<Command, ProtocolError> {
    reject(wire, true, true, false, false)?;
    Ok(constructor(
        required_package(wire)?,
        wire.slot.clone().ok_or(ProtocolError::MissingSlot)?,
    ))
}

fn required_package(wire: &WireRequest) -> Result<PackageName, ProtocolError> {
    wire.package.clone().ok_or(ProtocolError::MissingPackage)
}

#[allow(
    clippy::fn_params_excessive_bools,
    reason = "four wire fields are validated independently against a fixed command schema"
)]
const fn reject(
    wire: &WireRequest,
    package: bool,
    slot: bool,
    display_name: bool,
    seed_mode: bool,
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
    Ok(())
}
