use serde::{Deserialize, Serialize};

#[doc = "Android per-user application enabled setting captured before gate acquisition."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageEnabledState {
    #[doc = "The package follows its manifest default."]
    Default,
    #[doc = "The package was explicitly enabled."]
    Enabled,
    #[doc = "The package was explicitly disabled."]
    Disabled,
    #[doc = "The package was disabled by the Android user."]
    DisabledUser,
    #[doc = "The package remains disabled until explicitly used."]
    DisabledUntilUsed,
}

#[doc = "Exact package state that must be restored after a verified transaction."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateSnapshot {
    enabled_state: PackageEnabledState,
    suspended: bool,
}

impl GateSnapshot {
    #[doc = "Captures the enabled and suspended state before runtime gating."]
    pub const fn new(enabled_state: PackageEnabledState, suspended: bool) -> Self {
        Self {
            enabled_state,
            suspended,
        }
    }

    #[doc = "Returns the exact enabled setting captured before gating."]
    pub const fn enabled_state(self) -> PackageEnabledState {
        self.enabled_state
    }

    #[doc = "Returns whether another authority had already suspended the package."]
    pub const fn suspended(self) -> bool {
        self.suspended
    }
}
