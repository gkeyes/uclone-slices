use serde::{Deserialize, Serialize};

use crate::domain::PackageName;

/// Version of the boot-time emergency manifest wire record.
pub const SCHEMA_VERSION: u32 = 3;

/// Trustworthiness of the package-discovery evidence used to build one manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryIntegrity {
    /// Discovery completed and every attributable package fact was represented.
    Complete,
    /// Discovery was incomplete, corrupt, or could not attribute every safety fact.
    Untrusted,
}

/// Containment obligation for one package during boot convergence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentObligation {
    /// Keep the package held by the emergency gate.
    Held,
    /// Keep the package held and require recovery before release.
    HeldRecovery,
    /// Let the emergency watcher defer only while a live typed Runtime owner proof matches.
    ///
    /// This state does not prove that the package is enabled or that starting it is safe.
    RuntimeOwned,
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
            Self::RuntimeOwned => "runtime_owned",
            Self::BaseRetired => "base_retired",
            Self::NotManaged => "not_managed",
        }
    }

    /// Returns whether an obligation may follow during the same boot and enrollment epoch.
    pub const fn can_transition_same_boot_to(self, next: Self) -> bool {
        match self {
            Self::Held => matches!(
                next,
                Self::Held | Self::HeldRecovery | Self::RuntimeOwned | Self::BaseRetired
            ),
            Self::HeldRecovery => matches!(
                next,
                Self::HeldRecovery | Self::RuntimeOwned | Self::BaseRetired
            ),
            Self::RuntimeOwned => {
                matches!(next, Self::RuntimeOwned | Self::Held | Self::HeldRecovery)
            }
            Self::BaseRetired => matches!(next, Self::BaseRetired),
            Self::NotManaged => matches!(next, Self::NotManaged),
        }
    }

    /// Returns whether an obligation may seed the next boot without new side effects.
    pub const fn can_transition_new_boot_to(self, next: Self) -> bool {
        match self {
            Self::Held => matches!(next, Self::Held | Self::HeldRecovery),
            Self::HeldRecovery => matches!(next, Self::HeldRecovery),
            Self::RuntimeOwned => matches!(next, Self::Held | Self::HeldRecovery),
            Self::BaseRetired => matches!(next, Self::BaseRetired),
            Self::NotManaged => matches!(next, Self::NotManaged),
        }
    }
}

/// Aggregate boot disposition derived strictly from package obligations.
///
/// Mixed packages are ordered fail-closed as `HeldRecovery`, `Held`, `RuntimeOwned`,
/// `BaseRetired`, then `NotManaged`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallDisposition {
    /// At least one package is held without a recovery marker.
    Held,
    /// At least one package is held and requires recovery.
    HeldRecovery,
    /// At least one package is conditionally delegated to a proven live Runtime owner.
    ///
    /// Consumers must still validate that typed owner proof before deferring containment.
    RuntimeOwned,
    /// Every managed package has reached native Base.
    BaseRetired,
    /// No package is managed by the runtime.
    NotManaged,
}

impl OverallDisposition {
    const fn priority(self) -> u8 {
        match self {
            Self::NotManaged => 0,
            Self::BaseRetired => 1,
            Self::RuntimeOwned => 2,
            Self::Held => 3,
            Self::HeldRecovery => 4,
        }
    }
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

pub(super) fn derive_overall(
    discovery_integrity: DiscoveryIntegrity,
    packages: &[PackageContainment],
) -> OverallDisposition {
    let mut result = match discovery_integrity {
        DiscoveryIntegrity::Complete => OverallDisposition::NotManaged,
        DiscoveryIntegrity::Untrusted => OverallDisposition::HeldRecovery,
    };
    for package in packages {
        let candidate = match package.obligation {
            ContainmentObligation::HeldRecovery => OverallDisposition::HeldRecovery,
            ContainmentObligation::Held => OverallDisposition::Held,
            ContainmentObligation::RuntimeOwned => OverallDisposition::RuntimeOwned,
            ContainmentObligation::BaseRetired => OverallDisposition::BaseRetired,
            ContainmentObligation::NotManaged => OverallDisposition::NotManaged,
        };
        if candidate.priority() > result.priority() {
            result = candidate;
        }
    }
    result
}
