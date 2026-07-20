use serde::{Deserialize, Serialize};

use super::outcome::ReconcileOutcome;
use crate::domain::{PackageName, SlotId};
use crate::lifecycle::LifecycleState;

/// Probe capability result for the allowlisted package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeReport {
    package: PackageName,
    ready: bool,
    user_unlocked: bool,
    ce_de_supported: bool,
}

impl ProbeReport {
    /// Creates a bounded capability report.
    pub const fn new(
        package: PackageName,
        ready: bool,
        user_unlocked: bool,
        ce_de_supported: bool,
    ) -> Self {
        Self {
            package,
            ready,
            user_unlocked,
            ce_de_supported,
        }
    }

    /// Returns the reported package.
    pub const fn package(&self) -> &PackageName {
        &self.package
    }

    /// Returns whether all runtime gates are ready.
    pub const fn ready(&self) -> bool {
        self.ready
    }

    /// Returns whether user 0 CE is unlocked.
    pub const fn user_unlocked(&self) -> bool {
        self.user_unlocked
    }

    /// Returns whether paired CE and DE views are supported.
    pub const fn ce_de_supported(&self) -> bool {
        self.ce_de_supported
    }
}

/// Current allowlisted package status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageStatus {
    package: PackageName,
    slot: SlotId,
    lifecycle: LifecycleState,
    enabled: bool,
    suspended: bool,
}

impl PackageStatus {
    /// Creates a bounded package status report.
    pub const fn new(
        package: PackageName,
        slot: SlotId,
        lifecycle: LifecycleState,
        enabled: bool,
        suspended: bool,
    ) -> Self {
        Self {
            package,
            slot,
            lifecycle,
            enabled,
            suspended,
        }
    }

    /// Returns the reported package.
    pub const fn package(&self) -> &PackageName {
        &self.package
    }

    /// Returns the active logical slot.
    pub const fn slot(&self) -> &SlotId {
        &self.slot
    }

    /// Returns the persisted lifecycle state.
    pub const fn lifecycle(&self) -> LifecycleState {
        self.lifecycle
    }

    /// Returns the Android enabled state.
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// Returns whether another authority suspended the package.
    pub const fn suspended(&self) -> bool {
        self.suspended
    }
}

/// Verified result of switching one package to one slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SwitchResult {
    package: PackageName,
    slot: SlotId,
}

impl SwitchResult {
    /// Creates a verified switch result.
    pub const fn new(package: PackageName, slot: SlotId) -> Self {
        Self { package, slot }
    }

    /// Returns the switched package.
    pub const fn package(&self) -> &PackageName {
        &self.package
    }

    /// Returns the verified active slot.
    pub const fn slot(&self) -> &SlotId {
        &self.slot
    }
}

/// Result of reconciling one allowlisted package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconcileReport {
    package: PackageName,
    outcome: ReconcileOutcome,
}

impl ReconcileReport {
    /// Creates a typed reconciliation result.
    pub const fn new(package: PackageName, outcome: ReconcileOutcome) -> Self {
        Self { package, outcome }
    }

    /// Returns the reconciled package.
    pub const fn package(&self) -> &PackageName {
        &self.package
    }

    /// Returns the bounded reconciliation outcome.
    pub const fn outcome(&self) -> &ReconcileOutcome {
        &self.outcome
    }
}

/// Commands that can complete with an acknowledgement payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AckOperation {
    /// Base enrollment was durably published.
    EnrollPackage,
    /// Base-only rescue was durably completed.
    RescueToBase,
}

/// Typed acknowledgement for an operation without a richer report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ack {
    operation: AckOperation,
}

impl Ack {
    /// Creates a typed acknowledgement.
    pub const fn new(operation: AckOperation) -> Self {
        Self { operation }
    }

    /// Returns the acknowledged operation.
    pub const fn operation(&self) -> AckOperation {
        self.operation
    }
}
