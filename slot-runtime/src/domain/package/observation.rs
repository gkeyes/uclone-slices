use super::AppIdentity;
use crate::domain::DataInodes;

#[doc = "One consistent sample of `PackageManager` and mounted data views."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageObservation {
    identity: AppIdentity,
    package_manager_inodes: DataInodes,
    canonical_inodes: DataInodes,
    active_process_inodes: DataInodes,
    pending_install: bool,
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
}
