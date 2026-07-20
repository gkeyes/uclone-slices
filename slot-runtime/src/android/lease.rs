use crate::domain::{DataInodes, GateSnapshot, ManagedPackage, PackageName, UserId};

mod codec;
mod discovery;
mod file;
mod filesystem;
mod retirement;

pub use file::FileGateLeaseStore;

#[doc = "Exact enrolled rescue state durably published before the package gate is acquired."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateLease {
    package_name: PackageName,
    user_id: UserId,
    snapshot: GateSnapshot,
    base_inodes: DataInodes,
}

impl GateLease {
    pub(super) fn capture(package: &ManagedPackage, snapshot: GateSnapshot) -> Self {
        Self {
            package_name: package.package_name().clone(),
            user_id: package.user_id(),
            snapshot,
            base_inodes: package.base_inodes(),
        }
    }

    #[doc = "Returns the allowlisted package recorded by the lease."]
    pub const fn package_name(&self) -> &PackageName {
        &self.package_name
    }

    #[doc = "Returns the Android user recorded by the lease."]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[doc = "Returns the exact enabled and suspended state to restore."]
    pub const fn snapshot(&self) -> GateSnapshot {
        self.snapshot
    }

    #[doc = "Returns the enrolled native CE/DE inode anchors."]
    pub const fn base_inodes(&self) -> DataInodes {
        self.base_inodes
    }

    pub(super) fn validate_for(&self, package: &ManagedPackage) -> Result<(), &'static str> {
        if self.package_name() != package.package_name() || self.user_id() != package.user_id() {
            return Err("gate_lease_identity_mismatch");
        }
        if self.base_inodes() != package.base_inodes() {
            return Err("gate_lease_base_mismatch");
        }
        Ok(())
    }
}

#[doc = "Exact gate state captured before enrollment anchors can be trusted."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmergencyGateLease {
    package_name: PackageName,
    user_id: UserId,
    snapshot: GateSnapshot,
    phase: EmergencyGatePhase,
}

#[doc = "Durable phase of a preliminary emergency gate lease."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmergencyGatePhase {
    #[doc = "The exact snapshot is prepared, but no gate mutation is proven."]
    Prepared,
    #[doc = "Disable and process quiescence were proven after the snapshot."]
    Held,
}

impl EmergencyGateLease {
    pub(super) fn capture(package_name: &PackageName, snapshot: GateSnapshot) -> Self {
        Self {
            package_name: package_name.clone(),
            user_id: UserId::PRIMARY,
            snapshot,
            phase: EmergencyGatePhase::Prepared,
        }
    }

    #[doc = "Returns the allowlisted package recorded by the preliminary lease."]
    pub const fn package_name(&self) -> &PackageName {
        &self.package_name
    }

    #[doc = "Returns the fixed Android user recorded by the preliminary lease."]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[doc = "Returns the exact enabled and suspended state captured before gating."]
    pub const fn snapshot(&self) -> GateSnapshot {
        self.snapshot
    }

    #[doc = "Returns whether the disable and quiescence proof was durably committed."]
    pub const fn phase(&self) -> EmergencyGatePhase {
        self.phase
    }

    #[doc = "Returns a copy marked Held after the gate proof completes."]
    #[must_use]
    pub fn held(&self) -> Self {
        Self {
            package_name: self.package_name.clone(),
            user_id: self.user_id,
            snapshot: self.snapshot,
            phase: EmergencyGatePhase::Held,
        }
    }
}

#[doc = "Decoded durable gate state, typed by whether enrollment anchors were confirmed."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredGateLease {
    #[doc = "A fail-closed emergency lease whose serialized inode fields are both zero."]
    Emergency(EmergencyGateLease),
    #[doc = "A restore-capable lease with validated non-zero enrollment anchors."]
    Enrolled(GateLease),
}

impl StoredGateLease {
    #[doc = "Returns the package recorded by either lease state."]
    pub const fn package_name(&self) -> &PackageName {
        match self {
            Self::Emergency(lease) => lease.package_name(),
            Self::Enrolled(lease) => lease.package_name(),
        }
    }

    #[doc = "Returns the fixed Android user recorded by either lease state."]
    pub const fn user_id(&self) -> UserId {
        match self {
            Self::Emergency(lease) => lease.user_id(),
            Self::Enrolled(lease) => lease.user_id(),
        }
    }

    #[doc = "Returns the original exact package gate state."]
    pub const fn snapshot(&self) -> GateSnapshot {
        match self {
            Self::Emergency(lease) => lease.snapshot(),
            Self::Enrolled(lease) => lease.snapshot(),
        }
    }

    #[doc = "Returns enrolled anchors, or none while the lease remains preliminary."]
    pub const fn base_inodes(&self) -> Option<DataInodes> {
        match self {
            Self::Emergency(_) => None,
            Self::Enrolled(lease) => Some(lease.base_inodes()),
        }
    }
}

#[doc = "Failure to durably publish, decode, upgrade, or retire a rescue gate lease."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GateLeaseError {
    #[doc = "The fixed gate-state filesystem could not complete the operation."]
    #[error("gate lease store unavailable")]
    Unavailable,
    #[doc = "The artifact contents did not form one exact typed lease record."]
    #[error("gate lease artifact is invalid")]
    InvalidArtifact,
    #[doc = "The published artifact was not a root-owned mode-0600 regular file."]
    #[error("gate lease artifact ownership or mode is unsafe")]
    UnsafeArtifact,
}

#[doc = "Injected durability boundary for `KernelSU` rescue gate state."]
pub trait GateLeaseStore: core::fmt::Debug {
    #[doc = "Enumerates recognizable active or transitional lease package names."]
    fn package_names(&mut self) -> Result<Vec<PackageName>, GateLeaseError> {
        Ok(Vec::new())
    }

    #[doc = "Checks only whether a fixed lease or retirement artifact exists, without decoding it."]
    fn artifact_exists(&mut self, package: &PackageName) -> Result<bool, GateLeaseError>;

    #[doc = "Loads and validates an existing immutable lease when present."]
    fn load(&mut self, package: &PackageName) -> Result<Option<StoredGateLease>, GateLeaseError>;

    #[doc = "Atomically publishes a new preliminary zero-anchor lease."]
    fn persist_emergency(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError>;

    #[doc = "Atomically marks a prepared emergency lease Held after gate proof."]
    fn mark_emergency_held(&mut self, lease: &EmergencyGateLease) -> Result<(), GateLeaseError>;

    #[doc = "Atomically publishes a new restore-capable enrolled lease."]
    fn persist_enrolled(&mut self, lease: &GateLease) -> Result<(), GateLeaseError>;

    #[doc = "Atomically upgrades the matching preliminary lease with enrollment anchors."]
    fn confirm_enrollment(&mut self, lease: &GateLease) -> Result<(), GateLeaseError>;

    #[doc = "Retires only the exact lease whose package state was just proved restored."]
    fn retire(&mut self, expected: &GateLease) -> Result<(), GateLeaseError>;
}
