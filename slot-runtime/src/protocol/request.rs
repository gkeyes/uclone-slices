use serde::{Deserialize, Serialize, Serializer, de::Deserializer};

use super::ProtocolError;
use crate::domain::{PackageName, SlotId};
use crate::slot_metadata::{SlotDisplayName, SlotSeedMode};

mod frame;
mod wire;
pub use frame::{decode_request, encode_request};

#[doc = "Bounded id echoed by every request and response."]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RequestId(String);

impl RequestId {
    #[doc = "Creates a request id safe to echo and use in logs."]
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

    #[doc = "Returns the wire representation."]
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

#[doc = "Typed command set accepted by the multi-app Preview runtime."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    #[doc = "Probe runtime and device capability without selecting a package."]
    Probe,
    #[doc = "Inspect one installed package before enrollment."]
    InspectPackage {
        #[doc = "Installed Android package selected by `PackageManager`."]
        package: PackageName,
    },
    #[doc = "List all durable managed-app enrollments."]
    ListManagedApps,
    #[doc = "List only fixed-root packages eligible for recovery-only Base rescue."]
    ListRecoveryTargets,
    #[doc = "Publish one package's immutable base enrollment."]
    EnrollPackage {
        #[doc = "Installed Android package selected by `PackageManager`."]
        package: PackageName,
        #[doc = "Explicit user acceptance for unlocked-only Direct Boot support."]
        accept_direct_boot_conditional: bool,
    },
    #[doc = "Read one package's committed view and lifecycle status."]
    StatusPackage {
        #[doc = "Durably enrolled Android package."]
        package: PackageName,
    },
    #[doc = "Read one coherent package status and slot snapshot."]
    PackageSnapshot {
        #[doc = "Durably enrolled Android package."]
        package: PackageName,
    },
    #[doc = "Create and activate a runtime-generated non-base slot."]
    CreateSlot {
        #[doc = "Durably enrolled Android package."]
        package: PackageName,
        #[doc = "Display-only label that never participates in filesystem paths."]
        display_name: SlotDisplayName,
        #[doc = "Whether the initial slot tree is blank or copied from Base."]
        seed_mode: SlotSeedMode,
    },
    #[doc = "List Base and all non-deleted package slots."]
    ListSlots {
        #[doc = "Durably enrolled Android package."]
        package: PackageName,
    },
    #[doc = "Switch one managed package to a validated slot."]
    Switch {
        #[doc = "Durably enrolled Android package."]
        package: PackageName,
        #[doc = "Runtime-issued slot identifier, including the reserved Base id."]
        slot: SlotId,
    },
    #[doc = "Open the already-committed slot only after an explicit client confirmation."]
    LaunchCurrent {
        #[doc = "Durably enrolled Android package."]
        package: PackageName,
        #[doc = "Client-confirmed active slot; a mismatch is rejected without launching."]
        expected_slot: SlotId,
    },
    #[doc = "Change only a slot's display label."]
    RenameSlot {
        #[doc = "Durably enrolled Android package."]
        package: PackageName,
        #[doc = "Runtime-issued non-base slot identifier."]
        slot: SlotId,
        #[doc = "Replacement display-only label."]
        display_name: SlotDisplayName,
    },
    #[doc = "Delete a non-active, non-base slot."]
    DeleteSlot {
        #[doc = "Durably enrolled Android package."]
        package: PackageName,
        #[doc = "Runtime-issued inactive non-base slot identifier."]
        slot: SlotId,
    },
    #[doc = "Reconcile all durable package streams."]
    Reconcile,
    #[doc = "Reconcile one durable package stream."]
    ReconcilePackage {
        #[doc = "Durably enrolled Android package."]
        package: PackageName,
    },
    #[doc = "Retire one package after a verified native-base rescue."]
    RetirePackage {
        #[doc = "Durably enrolled Android package to return to unmanaged Base."]
        package: PackageName,
    },
    #[doc = "Return one package to its immutable native Base view."]
    RescueToBase {
        #[doc = "Known package whose native Base view must be restored."]
        package: PackageName,
    },
}

#[doc = "A validated protocol request."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    request_id: RequestId,
    command: Command,
}

impl Request {
    #[doc = "Constructs a schema-current request from typed fields."]
    pub fn new(request_id: RequestId, command: Command) -> Result<Self, ProtocolError> {
        let request = Self {
            request_id,
            command,
        };
        request.validate()?;
        Ok(request)
    }

    #[doc = "Decodes exactly one bounded JSON-lines request frame."]
    pub fn from_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_request(frame)
    }

    #[doc = "Returns the request id."]
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }

    #[doc = "Returns the typed command."]
    pub const fn command(&self) -> &Command {
        &self.command
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        match &self.command {
            Command::RenameSlot { slot, .. } | Command::DeleteSlot { slot, .. }
                if slot.is_base() =>
            {
                Err(ProtocolError::UnexpectedField("base_slot"))
            }
            _ => Ok(()),
        }
    }
}

impl Serialize for Request {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        wire::serialize(&self.request_id, &self.command, serializer)
    }
}

impl<'de> Deserialize<'de> for Request {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = wire::WireRequest::deserialize(deserializer)?;
        wire::from_wire(wire).map_err(serde::de::Error::custom)
    }
}
