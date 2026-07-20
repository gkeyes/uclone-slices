use crate::domain::{DataInodes, GateSnapshot, PackageName, PackageObservation, SlotId, UserId};

#[doc = "Number of mountinfo entries targeting the canonical CE and DE paths."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountCounts {
    ce: u32,
    de: u32,
}

impl MountCounts {
    #[doc = "Constructs one CE/DE mount-count sample."]
    pub const fn new(ce: u32, de: u32) -> Self {
        Self { ce, de }
    }

    #[doc = "Returns the canonical CE mount count."]
    pub const fn ce(self) -> u32 {
        self.ce
    }

    #[doc = "Returns the canonical DE mount count."]
    pub const fn de(self) -> u32 {
        self.de
    }
}

#[doc = "Read-only proof that the daemon shares the validated global mount namespace."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MountNamespaceProof {
    daemon_namespace: u64,
    global_namespace: u64,
}

impl MountNamespaceProof {
    #[doc = "Captures the daemon and PID 1 mount-namespace identifiers."]
    pub const fn new(daemon_namespace: u64, global_namespace: u64) -> Self {
        Self {
            daemon_namespace,
            global_namespace,
        }
    }

    #[doc = "Returns true only for equal, non-zero namespace identifiers."]
    pub const fn is_global(self) -> bool {
        self.daemon_namespace != 0
            && self.global_namespace != 0
            && self.daemon_namespace == self.global_namespace
    }
}

#[doc = "Canonical CE/DE inode and mount-count sample from the mount-master namespace."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalView {
    inodes: DataInodes,
    mount_counts: MountCounts,
}

impl CanonicalView {
    #[doc = "Combines canonical CE/DE inodes with their mount counts."]
    pub const fn new(inodes: DataInodes, mount_counts: MountCounts) -> Self {
        Self {
            inodes,
            mount_counts,
        }
    }

    #[doc = "Returns the canonical CE/DE inodes."]
    pub const fn inodes(self) -> DataInodes {
        self.inodes
    }

    #[doc = "Returns the canonical CE/DE mount counts."]
    pub const fn mount_counts(self) -> MountCounts {
        self.mount_counts
    }
}

#[doc = "One coherent canonical, mirror, and Zygote view observation."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewProof {
    canonical: CanonicalView,
    mirror_inodes: DataInodes,
    zygote_inodes: DataInodes,
}

impl ViewProof {
    #[doc = "Combines the three namespace inode views and canonical mount counts."]
    pub const fn new(
        canonical: CanonicalView,
        mirror_inodes: DataInodes,
        zygote_inodes: DataInodes,
    ) -> Self {
        Self {
            canonical,
            mirror_inodes,
            zygote_inodes,
        }
    }

    #[doc = "Returns the mount-master canonical observation."]
    pub const fn canonical(self) -> CanonicalView {
        self.canonical
    }

    #[doc = "Returns the data-mirror CE/DE inodes."]
    pub const fn mirror_inodes(self) -> DataInodes {
        self.mirror_inodes
    }

    #[doc = "Returns the Zygote namespace CE/DE inodes."]
    pub const fn zygote_inodes(self) -> DataInodes {
        self.zygote_inodes
    }
}

#[doc = "Failure to obtain a trustworthy package or filesystem observation."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProbeError {
    #[doc = "The observation source was unavailable."]
    #[error("package probe unavailable")]
    Unavailable,
    #[doc = "The observation source returned an invalid or incomplete sample."]
    #[error("package probe returned an invalid response")]
    InvalidResponse,
}

impl ProbeError {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::Unavailable => "probe_unavailable",
            Self::InvalidResponse => "probe_invalid_response",
        }
    }
}

#[doc = "Injected read-only boundary for Android identity, inode, gate, and process facts."]
pub trait PackageProbe: core::fmt::Debug {
    #[doc = "Compares the daemon mount namespace with the validated PID 1 namespace."]
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError>;

    #[doc = "Samples identity, `PackageManager` inodes, canonical inodes, and process inodes."]
    fn observe_package(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<PackageObservation, ProbeError>;

    #[doc = "Samples the exact enabled and suspended state for one package."]
    fn gate_snapshot(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<GateSnapshot, ProbeError>;

    #[doc = "Counts all running processes that belong to one package UID."]
    fn running_process_count(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<u32, ProbeError>;

    #[doc = "Samples canonical, mirror, and Zygote inodes plus exact canonical mount counts."]
    fn view_proof(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<ViewProof, ProbeError>;

    #[doc = "Samples the CE/DE inodes of one derived non-base slot source."]
    fn slot_inodes(
        &mut self,
        package: &PackageName,
        slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError>;

    #[doc = "Returns whether Android user zero's credential storage is unlocked."]
    fn user0_unlocked(&mut self) -> Result<bool, ProbeError>;
}
