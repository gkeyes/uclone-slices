use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use super::{BridgeError, invalid_response};

#[doc = "Device unlock facts returned by the fixed bridge."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSnapshot {
    user_id: u32,
    unlocked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    api_level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    build_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeviceSnapshotWire {
    user_id: u32,
    unlocked: bool,
    #[serde(default)]
    api_level: Option<u32>,
    #[serde(default)]
    build_fingerprint: Option<String>,
}

impl<'de> Deserialize<'de> for DeviceSnapshot {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_wire(DeviceSnapshotWire::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

impl DeviceSnapshot {
    #[doc = "Constructs a minimal device snapshot."]
    pub fn new(user_id: u32, unlocked: bool) -> Result<Self, BridgeError> {
        Self::from_wire(DeviceSnapshotWire {
            user_id,
            unlocked,
            api_level: None,
            build_fingerprint: None,
        })
    }

    #[doc = "Constructs a snapshot with bounded optional build metadata."]
    pub fn with_metadata(
        user_id: u32,
        unlocked: bool,
        api_level: Option<u32>,
        build_fingerprint: Option<&str>,
    ) -> Result<Self, BridgeError> {
        Self::from_wire(DeviceSnapshotWire {
            user_id,
            unlocked,
            api_level,
            build_fingerprint: build_fingerprint.map(str::to_owned),
        })
    }

    fn from_wire(wire: DeviceSnapshotWire) -> Result<Self, BridgeError> {
        if wire.api_level == Some(0) {
            return Err(invalid_response("device apiLevel must be non-zero"));
        }
        if let Some(fingerprint) = wire.build_fingerprint.as_deref() {
            super::validation::validate_text(fingerprint, 256, false, "buildFingerprint")?;
        }
        Ok(Self {
            user_id: wire.user_id,
            unlocked: wire.unlocked,
            api_level: wire.api_level,
            build_fingerprint: wire.build_fingerprint,
        })
    }

    #[doc = "Returns the Android user id reported by the bridge."]
    pub const fn user_id(&self) -> u32 {
        self.user_id
    }

    #[doc = "Returns whether credential-encrypted storage is unlocked."]
    pub const fn unlocked(&self) -> bool {
        self.unlocked
    }

    #[doc = "Returns the optional Android API level."]
    pub const fn api_level(&self) -> Option<u32> {
        self.api_level
    }

    #[doc = "Returns the optional build fingerprint."]
    pub fn build_fingerprint(&self) -> Option<&str> {
        self.build_fingerprint.as_deref()
    }
}
