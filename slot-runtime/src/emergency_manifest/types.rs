use serde::{Deserialize, Serialize};

use crate::domain::PackageName;

/// Version of the boot-time emergency manifest wire record.
pub const SCHEMA_VERSION: u32 = 2;

/// Containment obligation for one package during boot convergence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentObligation {
    /// Keep the package held by the emergency gate.
    Held,
    /// Keep the package held and require recovery before release.
    HeldRecovery,
    /// The package has been retired to native Base.
    BaseRetired,
    /// The package is outside the managed set and needs no containment.
    NotManaged,
}

impl ContainmentObligation {
    /// Returns the stable serialized name of this obligation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::HeldRecovery => "held_recovery",
            Self::BaseRetired => "base_retired",
            Self::NotManaged => "not_managed",
        }
    }

    /// Returns whether an obligation may follow this proven obligation.
    pub const fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Held => matches!(next, Self::Held | Self::HeldRecovery | Self::BaseRetired),
            Self::HeldRecovery => matches!(next, Self::HeldRecovery | Self::BaseRetired),
            Self::BaseRetired => matches!(next, Self::BaseRetired),
            Self::NotManaged => {
                matches!(next, Self::NotManaged | Self::Held | Self::HeldRecovery)
            }
        }
    }
}

/// Aggregate boot disposition derived strictly from package obligations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallDisposition {
    /// At least one package is held without a recovery marker.
    Held,
    /// At least one package is held and requires recovery.
    HeldRecovery,
    /// Every managed package has reached native Base.
    BaseRetired,
    /// No package is managed by the runtime.
    NotManaged,
}

/// One sorted package-to-containment obligation entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageContainment {
    package: PackageName,
    enrollment_epoch: u64,
    obligation: ContainmentObligation,
}

impl PackageContainment {
    /// Creates one package entry from an already validated package name.
    pub const fn new(package: PackageName, obligation: ContainmentObligation) -> Self {
        Self::with_enrollment_epoch(package, 1, obligation)
    }

    /// Creates an entry fenced to an explicit enrollment epoch.
    pub const fn with_enrollment_epoch(
        package: PackageName,
        enrollment_epoch: u64,
        obligation: ContainmentObligation,
    ) -> Self {
        Self {
            package,
            enrollment_epoch,
            obligation,
        }
    }

    /// Returns the package identity.
    pub const fn package(&self) -> &PackageName {
        &self.package
    }

    /// Returns the monotonically increasing enrollment epoch.
    pub const fn enrollment_epoch(&self) -> u64 {
        self.enrollment_epoch
    }

    /// Returns the package's required containment obligation.
    pub const fn obligation(&self) -> ContainmentObligation {
        self.obligation
    }
}

pub(super) fn derive_overall(packages: &[PackageContainment]) -> OverallDisposition {
    let mut result = OverallDisposition::NotManaged;
    for package in packages {
        result = match package.obligation {
            ContainmentObligation::HeldRecovery => OverallDisposition::HeldRecovery,
            ContainmentObligation::Held if result != OverallDisposition::HeldRecovery => {
                OverallDisposition::Held
            }
            ContainmentObligation::Held => result,
            ContainmentObligation::BaseRetired
                if matches!(
                    result,
                    OverallDisposition::NotManaged | OverallDisposition::BaseRetired
                ) =>
            {
                OverallDisposition::BaseRetired
            }
            ContainmentObligation::BaseRetired => result,
            ContainmentObligation::NotManaged => result,
        };
    }
    result
}
