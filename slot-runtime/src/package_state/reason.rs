use serde::{Deserialize, Serialize};

#[doc = "Stable machine-readable cause attached to a lifecycle-state revision."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageStateReason {
    #[doc = "The immutable enrollment was initialized in the normal state."]
    Enrolled,
    #[doc = "A managed update advanced through its ordinary lifecycle phases."]
    ManagedUpdate,
    #[doc = "Package identity changed outside the enrolled contract."]
    IdentityChanged,
    #[doc = "The runtime cannot prove a coherent visible package view."]
    ViewUncertain,
    #[doc = "A lifecycle invariant drifted outside its managed transition."]
    LifecycleDrift,
    #[doc = "An operator-approved repair changed the durable lifecycle state."]
    ManualRepair,
}
