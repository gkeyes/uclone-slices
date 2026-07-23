use serde::{Deserialize, Serialize};

use crate::domain::BootId;

/// Runtime role authorized to own containment coordination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOwnerRole {
    /// The privileged UClone Slots daemon.
    Ucloned,
}

/// Exact live daemon incarnation that may temporarily own containment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeOwnerProof {
    boot_id: BootId,
    pid: u32,
    start_ticks: u64,
    role: RuntimeOwnerRole,
    lock_device: u64,
    lock_inode: u64,
}

impl RuntimeOwnerProof {
    /// Captures the typed process and lock identity that consumers must revalidate live.
    pub const fn new(
        boot_id: BootId,
        pid: u32,
        start_ticks: u64,
        role: RuntimeOwnerRole,
        lock_device: u64,
        lock_inode: u64,
    ) -> Self {
        Self {
            boot_id,
            pid,
            start_ticks,
            role,
            lock_device,
            lock_inode,
        }
    }

    /// Returns the boot epoch of the claimed owner.
    pub const fn boot_id(&self) -> &BootId {
        &self.boot_id
    }

    /// Returns the claimed daemon process identifier.
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    /// Returns the `/proc/<pid>/stat` process start time.
    pub const fn start_ticks(&self) -> u64 {
        self.start_ticks
    }

    /// Returns the only daemon role allowed to own containment.
    pub const fn role(&self) -> RuntimeOwnerRole {
        self.role
    }

    /// Returns the device containing the authoritative Runtime lock.
    pub const fn lock_device(&self) -> u64 {
        self.lock_device
    }

    /// Returns the inode of the authoritative Runtime lock.
    pub const fn lock_inode(&self) -> u64 {
        self.lock_inode
    }
}
