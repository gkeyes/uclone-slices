use serde::{Deserialize, Serialize};

use super::{DeviceSnapshot, PackageEnabledState, PackageSnapshot};
use crate::domain::GateSnapshot;

#[doc = "Empty acknowledgement payload reserved for fixed bridge mutations."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AckPayload {}

#[doc = "Lightweight PackageManager gate state returned without app-data metadata."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGateSnapshot {
    enabled_state: PackageEnabledState,
    suspended: bool,
}

impl BridgeGateSnapshot {
    #[doc = "Converts the validated payload into the runtime gate snapshot."]
    pub const fn into_gate_snapshot(self) -> GateSnapshot {
        GateSnapshot::new(self.enabled_state, self.suspended)
    }
}

#[doc = "Tagged success payload returned by the fixed Java bridge."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BridgePayload {
    #[doc = "Device unlock response."]
    Device(DeviceSnapshot),
    #[doc = "Package state response."]
    Package(PackageSnapshot),
    #[doc = "Lightweight package execution-gate response."]
    Gate(BridgeGateSnapshot),
    #[doc = "Fixed acknowledgement response."]
    Ack(AckPayload),
}
