use serde::{Deserialize, Serialize, Serializer, de::Deserializer};

use super::{ALLOWED_PACKAGE, ProtocolError, SCHEMA_VERSION};
use crate::domain::{PackageName, SlotId};
use crate::target::{BASE_SLOT, PREVIEW_SLOT};

mod frame;
pub use frame::{decode_request, encode_request};

/// Bounded id echoed by every request and response.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RequestId(String);

impl RequestId {
    /// Creates a request id safe to echo and use in logs.
    pub fn new(value: &str) -> Result<Self, ProtocolError> {
        if (1..=128).contains(&value.len())
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
        {
            Ok(Self(value.to_owned()))
        } else {
            Err(ProtocolError::InvalidRequestId(value.to_owned()))
        }
    }

    /// Returns the wire representation.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RequestId {
    type Error = ProtocolError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<RequestId> for String {
    fn from(value: RequestId) -> Self {
        value.0
    }
}

/// Fixed command set accepted by the Preview runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Probe runtime/device capabilities without selecting a package.
    Probe,
    /// Publish the allowlisted package's immutable base enrollment.
    EnrollPackage {
        /// Package to enroll.
        package: PackageName,
    },
    /// Read the allowlisted package's current status.
    StatusPackage {
        /// Package whose status is requested.
        package: PackageName,
    },
    /// Switch the allowlisted package to a validated logical slot.
    Switch {
        /// Package to switch.
        package: PackageName,
        /// Logical destination slot.
        slot: SlotId,
    },
    /// Reconcile any durable transaction left by an interrupted operation.
    Reconcile,
    /// Return the allowlisted package to its immutable base slot.
    RescueToBase {
        /// Package to rescue.
        package: PackageName,
    },
}

/// A validated protocol request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    request_id: RequestId,
    command: Command,
}

impl Request {
    /// Constructs a schema-current request and applies the package allowlist.
    pub fn new(request_id: RequestId, command: Command) -> Result<Self, ProtocolError> {
        let request = Self {
            request_id,
            command,
        };
        request.validate()?;
        Ok(request)
    }

    /// Decodes exactly one bounded JSON-lines request frame.
    pub fn from_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_request(frame)
    }

    /// Returns the request id.
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    /// Returns the fixed command.
    pub const fn command(&self) -> &Command {
        &self.command
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        match &self.command {
            Command::Probe | Command::Reconcile => Ok(()),
            Command::EnrollPackage { package }
            | Command::StatusPackage { package }
            | Command::RescueToBase { package } => validate_package(package),
            Command::Switch { package, slot } => {
                validate_package(package)?;
                validate_slot(slot)
            }
        }
    }
}

impl Serialize for Request {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (command, package, slot) = match &self.command {
            Command::Probe => ("probe", None, None),
            Command::EnrollPackage { package } => ("enroll_package", Some(package), None),
            Command::StatusPackage { package } => ("status_package", Some(package), None),
            Command::Switch { package, slot } => ("switch", Some(package), Some(slot)),
            Command::Reconcile => ("reconcile", None, None),
            Command::RescueToBase { package } => ("rescue_to_base", Some(package), None),
        };
        WireRequestOut {
            schema_version: SCHEMA_VERSION,
            request_id: &self.request_id,
            command,
            package,
            slot,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireRequest::deserialize(deserializer)?;
        from_wire(wire).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRequest {
    schema_version: u32,
    request_id: RequestId,
    command: String,
    #[serde(default)]
    package: Option<PackageName>,
    #[serde(default)]
    slot: Option<SlotId>,
}

#[derive(Debug, Serialize)]
struct WireRequestOut<'a> {
    schema_version: u32,
    request_id: &'a RequestId,
    command: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    package: Option<&'a PackageName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    slot: Option<&'a SlotId>,
}

fn from_wire(wire: WireRequest) -> Result<Request, ProtocolError> {
    if wire.schema_version != SCHEMA_VERSION {
        return Err(ProtocolError::UnsupportedSchema(wire.schema_version));
    }
    let command = match wire.command.as_str() {
        "probe" => {
            reject_fields(wire.package.is_some(), wire.slot.is_some())?;
            Command::Probe
        }
        "enroll_package" => Command::EnrollPackage {
            package: required_package(wire.package, wire.slot.is_some())?,
        },
        "status_package" => Command::StatusPackage {
            package: required_package(wire.package, wire.slot.is_some())?,
        },
        "switch" => Command::Switch {
            package: wire.package.ok_or(ProtocolError::MissingPackage)?,
            slot: wire.slot.ok_or(ProtocolError::MissingSlot)?,
        },
        "reconcile" => {
            reject_fields(wire.package.is_some(), wire.slot.is_some())?;
            Command::Reconcile
        }
        "rescue_to_base" => Command::RescueToBase {
            package: required_package(wire.package, wire.slot.is_some())?,
        },
        unknown => return Err(ProtocolError::UnknownCommand(unknown.to_owned())),
    };
    Request::new(wire.request_id, command)
}

fn required_package(
    package: Option<PackageName>,
    slot_present: bool,
) -> Result<PackageName, ProtocolError> {
    if slot_present {
        return Err(ProtocolError::UnexpectedField("slot"));
    }
    package.ok_or(ProtocolError::MissingPackage)
}

const fn reject_fields(package_present: bool, slot_present: bool) -> Result<(), ProtocolError> {
    if package_present {
        return Err(ProtocolError::UnexpectedField("package"));
    }
    if slot_present {
        return Err(ProtocolError::UnexpectedField("slot"));
    }
    Ok(())
}

fn validate_package(package: &PackageName) -> Result<(), ProtocolError> {
    if package.as_str() == ALLOWED_PACKAGE {
        Ok(())
    } else {
        Err(ProtocolError::PackageNotAllowed(package.to_string()))
    }
}

fn validate_slot(slot: &SlotId) -> Result<(), ProtocolError> {
    if matches!(slot.as_str(), BASE_SLOT | PREVIEW_SLOT) {
        Ok(())
    } else {
        Err(ProtocolError::SlotNotAllowed(slot.to_string()))
    }
}
