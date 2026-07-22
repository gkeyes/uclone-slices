use super::AppIdentity;
use crate::domain::DataInodes;
use serde::{Deserialize, Serialize};

#[doc = "Bounded support class for one installed package."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageSupportLevel {
    #[doc = "Ordinary user-zero App supported by the Preview contract."]
    Supported,
    #[doc = "Direct Boot App supported only after unlock and explicit confirmation."]
    DirectBootConditional,
    #[doc = "System or shared-UID package that cannot be managed safely."]
    Blocked,
}

#[doc = "Installed-package properties that bound the first multi-App Preview."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageCompatibility {
    system_app: bool,
    shared_uid: bool,
    direct_boot_aware: bool,
}

impl PackageCompatibility {
    #[doc = "Creates one compatibility sample from `PackageManager` facts."]
    pub const fn new(system_app: bool, shared_uid: bool, direct_boot_aware: bool) -> Self {
        Self {
            system_app,
            shared_uid,
            direct_boot_aware,
        }
    }

    #[doc = "Returns the compatible ordinary third-party App baseline."]
    pub const fn compatible() -> Self {
        Self::new(false, false, false)
    }

    #[doc = "Returns whether the package satisfies the first Preview contract."]
    pub const fn is_supported(self) -> bool {
        matches!(self.support_level(), PackageSupportLevel::Supported)
    }

    #[doc = "Classifies ordinary, conditional Direct Boot, and blocked packages."]
    pub const fn support_level(self) -> PackageSupportLevel {
        if self.system_app || self.shared_uid {
            PackageSupportLevel::Blocked
        } else if self.direct_boot_aware {
            PackageSupportLevel::DirectBootConditional
        } else {
            PackageSupportLevel::Supported
        }
    }

    #[doc = "Returns whether `PackageManager` marks this as a system App."]
    pub const fn system_app(self) -> bool {
        self.system_app
    }

    #[doc = "Returns whether this package participates in a shared UID."]
    pub const fn shared_uid(self) -> bool {
        self.shared_uid
    }

    #[doc = "Returns whether a declared component is Direct Boot aware."]
    pub const fn direct_boot_aware(self) -> bool {
        self.direct_boot_aware
    }
}

#[doc = "Lightweight `PackageManager` candidate independent of running-process namespaces."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageCandidate {
    identity: AppIdentity,
    package_manager_inodes: DataInodes,
    pending_install: bool,
    compatibility: PackageCompatibility,
}

impl PackageCandidate {
    #[doc = "Creates one package candidate from typed `PackageManager` facts."]
    pub const fn new(
        identity: AppIdentity,
        package_manager_inodes: DataInodes,
        pending_install: bool,
        compatibility: PackageCompatibility,
    ) -> Self {
        Self {
            identity,
            package_manager_inodes,
            pending_install,
            compatibility,
        }
    }

    #[doc = "Returns the installed package identity."]
    pub const fn identity(&self) -> &AppIdentity {
        &self.identity
    }
    #[doc = "Returns `PackageManager`'s CE/DE inode anchors."]
    pub const fn package_manager_inodes(&self) -> DataInodes {
        self.package_manager_inodes
    }
    #[doc = "Returns whether an installer session is still pending."]
    pub const fn pending_install(&self) -> bool {
        self.pending_install
    }
    #[doc = "Returns the sampled compatibility flags."]
    pub const fn compatibility(&self) -> PackageCompatibility {
        self.compatibility
    }
}

#[doc = "One consistent sample of `PackageManager` and mounted data views."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageObservation {
    identity: AppIdentity,
    package_manager_inodes: DataInodes,
    canonical_inodes: DataInodes,
    active_process_inodes: DataInodes,
    pending_install: bool,
    compatibility: PackageCompatibility,
}

impl PackageObservation {
    #[doc = "Constructs an observation from already non-zero inode pairs."]
    pub const fn new(
        identity: AppIdentity,
        package_manager_inodes: DataInodes,
        canonical_inodes: DataInodes,
        active_process_inodes: DataInodes,
        pending_install: bool,
    ) -> Self {
        Self {
            identity,
            package_manager_inodes,
            canonical_inodes,
            active_process_inodes,
            pending_install,
            compatibility: PackageCompatibility::compatible(),
        }
    }

    #[doc = "Constructs an observation with explicit package compatibility facts."]
    pub const fn with_compatibility(
        identity: AppIdentity,
        package_manager_inodes: DataInodes,
        canonical_inodes: DataInodes,
        active_process_inodes: DataInodes,
        pending_install: bool,
        compatibility: PackageCompatibility,
    ) -> Self {
        Self {
            identity,
            package_manager_inodes,
            canonical_inodes,
            active_process_inodes,
            pending_install,
            compatibility,
        }
    }

    #[doc = "Returns the currently installed package identity."]
    pub const fn identity(&self) -> &AppIdentity {
        &self.identity
    }

    #[doc = "Returns `PackageManager`'s persisted CE/DE inodes."]
    pub const fn package_manager_inodes(&self) -> DataInodes {
        self.package_manager_inodes
    }

    #[doc = "Returns canonical-path CE/DE inodes from the mount-master namespace."]
    pub const fn canonical_inodes(&self) -> DataInodes {
        self.canonical_inodes
    }

    #[doc = "Returns CE/DE inodes observed from a target App process namespace."]
    pub const fn active_process_inodes(&self) -> DataInodes {
        self.active_process_inodes
    }

    #[doc = "Returns whether an installer session is pending for the package."]
    pub const fn pending_install(&self) -> bool {
        self.pending_install
    }

    #[doc = "Returns the sampled package compatibility properties."]
    pub const fn compatibility(&self) -> PackageCompatibility {
        self.compatibility
    }
}
