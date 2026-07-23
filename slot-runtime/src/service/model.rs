use crate::domain::{ManagedPackage, SlotView};

mod read;
pub use read::{ManagedAppInfo, PackageInspection, SlotInfo};

pub use crate::rescue::RescueExecution;

/// Read-only device capability facts used to build a protocol probe report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilitySnapshot {
    ready: bool,
    user_unlocked: bool,
    ce_de_supported: bool,
    recovery_only: bool,
}

impl CapabilitySnapshot {
    /// Groups already-probed user-zero runtime capability facts.
    pub const fn new(
        ready: bool,
        user_unlocked: bool,
        ce_de_supported: bool,
        recovery_only: bool,
    ) -> Self {
        Self {
            ready,
            user_unlocked,
            ce_de_supported,
            recovery_only,
        }
    }

    /// Returns whether every runtime gate is ready.
    pub const fn ready(self) -> bool {
        self.ready
    }

    /// Returns whether user-zero CE storage is unlocked.
    pub const fn user_unlocked(self) -> bool {
        self.user_unlocked
    }

    /// Returns whether paired CE and DE views are supported.
    pub const fn ce_de_supported(self) -> bool {
        self.ce_de_supported
    }

    /// Returns whether this daemon exposes only bounded recovery operations.
    pub const fn recovery_only(self) -> bool {
        self.recovery_only
    }
}

/// Read-only Android gate facts shown by the status command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedGateState {
    enabled: bool,
    suspended: bool,
}

impl ObservedGateState {
    /// Groups the observed enabled and suspension facts.
    pub const fn new(enabled: bool, suspended: bool) -> Self {
        Self { enabled, suspended }
    }

    /// Returns whether Android currently permits package execution.
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// Returns whether another authority currently suspends the package.
    pub const fn suspended(self) -> bool {
        self.suspended
    }
}

/// Complete validated read model for one enrolled package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageSnapshot {
    managed: ManagedPackage,
    slots: Vec<SlotView>,
    gate: ObservedGateState,
}

impl PackageSnapshot {
    /// Groups durable package state, optional fixed Preview catalog view, and live gate facts.
    pub const fn new(
        managed: ManagedPackage,
        slots: Vec<SlotView>,
        gate: ObservedGateState,
    ) -> Self {
        Self {
            managed,
            slots,
            gate,
        }
    }

    /// Returns the committed package contract.
    pub const fn managed(&self) -> &ManagedPackage {
        &self.managed
    }

    /// Returns every verified non-base catalog view.
    pub fn slots(&self) -> &[SlotView] {
        &self.slots
    }

    /// Returns one verified catalog view by immutable slot id.
    pub fn slot(&self, slot: &crate::domain::SlotId) -> Option<&SlotView> {
        self.slots.iter().find(|view| view.slot_id() == slot)
    }

    /// Returns read-only Android gate facts.
    pub const fn gate(&self) -> ObservedGateState {
        self.gate
    }
}

/// Result of validating all durable stores needed to describe a package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackageState {
    /// No immutable enrollment exists.
    Absent,
    /// Enrollment, catalog, lifecycle state, Registry, and Journal agree.
    Ready(Box<PackageSnapshot>),
    /// State is missing, ambiguous, corrupt, or otherwise unprovable.
    RecoveryRequired,
    /// Installed identity no longer owns the immutable enrollment.
    Quarantined,
}

/// Bounded result returned by a runtime switch or base rescue coordinator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SwitchExecution {
    /// The returned view was proved, committed, and its exact gate was restored.
    Committed(SlotView),
    /// The previous view and exact gate were proved restored.
    RolledBack,
    /// The coordinator retained the execution gate for reconciliation.
    RecoveryRequired,
    /// Identity verification retained the execution gate permanently.
    Quarantined,
}
