use serde::{Deserialize, Serialize, Serializer};

use super::{Command, Request, RequestId};
use crate::domain::{PackageName, SlotId};
use crate::protocol::{ProtocolError, RUNTIME_BUILD_ID, SCHEMA_VERSION};
use crate::slot_metadata::{SlotDisplayName, SlotSeedMode};

mod validation;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WireRequest {
    schema_version: u32,
    request_id: RequestId,
    command: String,
    #[serde(default)]
    build_id: Option<String>,
    #[serde(default)]
    package: Option<PackageName>,
    #[serde(default)]
    slot: Option<SlotId>,
    #[serde(default)]
    display_name: Option<SlotDisplayName>,
    #[serde(default)]
    seed_mode: Option<SlotSeedMode>,
    #[serde(default)]
    accept_direct_boot_conditional: Option<bool>,
}

#[derive(Serialize)]
struct WireRequestOut<'a> {
    schema_version: u32,
    request_id: &'a RequestId,
    command: &'a str,
    build_id: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<&'a PackageName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<&'a SlotId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a SlotDisplayName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    seed_mode: Option<SlotSeedMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accept_direct_boot_conditional: Option<bool>,
}

pub(super) fn serialize<S: Serializer>(
    request_id: &RequestId,
    command: &Command,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let (name, package, slot, display_name, seed_mode, direct_boot) = fields(command);
    WireRequestOut {
        schema_version: SCHEMA_VERSION,
        request_id,
        command: name,
        build_id: RUNTIME_BUILD_ID,
        package,
        slot,
        display_name,
        seed_mode,
        accept_direct_boot_conditional: direct_boot,
    }
    .serialize(serializer)
}

type Fields<'a> = (
    &'static str,
    Option<&'a PackageName>,
    Option<&'a SlotId>,
    Option<&'a SlotDisplayName>,
    Option<SlotSeedMode>,
    Option<bool>,
);

const fn fields(command: &Command) -> Fields<'_> {
    match command {
        Command::Probe => ("probe", None, None, None, None, None),
        Command::InspectPackage { package } => {
            ("inspect_package", Some(package), None, None, None, None)
        }
        Command::ListManagedApps => ("list_managed_apps", None, None, None, None, None),
        Command::EnrollPackage {
            package,
            accept_direct_boot_conditional,
        } => (
            "enroll_package",
            Some(package),
            None,
            None,
            None,
            Some(*accept_direct_boot_conditional),
        ),
        Command::StatusPackage { package } => {
            ("status_package", Some(package), None, None, None, None)
        }
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
            None,
        ),
        Command::ListSlots { package } => ("list_slots", Some(package), None, None, None, None),
        Command::Switch { package, slot } => {
            ("switch", Some(package), Some(slot), None, None, None)
        }
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
            None,
        ),
        Command::DeleteSlot { package, slot } => {
            ("delete_slot", Some(package), Some(slot), None, None, None)
        }
        Command::Reconcile => ("reconcile", None, None, None, None, None),
        Command::ReconcilePackage { package } => {
            ("reconcile_package", Some(package), None, None, None, None)
        }
        Command::RetirePackage { package } => {
            ("retire_package", Some(package), None, None, None, None)
        }
        Command::RescueToBase { package } => {
            ("rescue_to_base", Some(package), None, None, None, None)
        }
    }
}

pub(super) fn from_wire(wire: WireRequest) -> Result<Request, ProtocolError> {
    if wire.schema_version != SCHEMA_VERSION {
        return Err(ProtocolError::UnsupportedSchema(wire.schema_version));
    }
    let command = match wire.command.as_str() {
        "probe" => validation::no_fields(&wire, Command::Probe)?,
        "inspect_package" => {
            validation::package_only(&wire, |package| Command::InspectPackage { package })?
        }
        "list_managed_apps" => validation::no_fields(&wire, Command::ListManagedApps)?,
        "enroll_package" => {
            validation::validate(&wire, validation::WireSchema::ENROLL)?;
            Command::EnrollPackage {
                package: validation::required_package(&wire)?,
                accept_direct_boot_conditional: wire
                    .accept_direct_boot_conditional
                    .unwrap_or(false),
            }
        }
        "status_package" => {
            validation::package_only(&wire, |package| Command::StatusPackage { package })?
        }
        "create_slot" => {
            validation::validate(&wire, validation::WireSchema::CREATE)?;
            Command::CreateSlot {
                package: validation::required_package(&wire)?,
                display_name: wire
                    .display_name
                    .clone()
                    .ok_or(ProtocolError::UnexpectedField("display_name"))?,
                seed_mode: wire
                    .seed_mode
                    .ok_or(ProtocolError::UnexpectedField("seed_mode"))?,
            }
        }
        "list_slots" => validation::package_only(&wire, |package| Command::ListSlots { package })?,
        "switch" => {
            validation::package_slot(&wire, |package, slot| Command::Switch { package, slot })?
        }
        "rename_slot" => {
            validation::validate(&wire, validation::WireSchema::RENAME)?;
            Command::RenameSlot {
                package: validation::required_package(&wire)?,
                slot: wire.slot.clone().ok_or(ProtocolError::MissingSlot)?,
                display_name: wire
                    .display_name
                    .clone()
                    .ok_or(ProtocolError::UnexpectedField("display_name"))?,
            }
        }
        "delete_slot" => {
            validation::package_slot(&wire, |package, slot| Command::DeleteSlot { package, slot })?
        }
        "reconcile" => validation::no_fields(&wire, Command::Reconcile)?,
        "reconcile_package" => {
            validation::package_only(&wire, |package| Command::ReconcilePackage { package })?
        }
        "retire_package" => {
            validation::package_only(&wire, |package| Command::RetirePackage { package })?
        }
        "rescue_to_base" => {
            validation::package_only(&wire, |package| Command::RescueToBase { package })?
        }
        unknown => return Err(ProtocolError::UnknownCommand(unknown.to_owned())),
    };
    if !matches!(&command, Command::Probe | Command::RescueToBase { .. })
        && wire.build_id.as_deref() != Some(RUNTIME_BUILD_ID)
    {
        return Err(ProtocolError::RuntimePairMismatch);
    }
    Request::new(wire.request_id, command)
}
