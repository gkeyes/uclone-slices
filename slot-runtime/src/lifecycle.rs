use serde::{Deserialize, Serialize};

use crate::domain::{InstalledArtifact, ManagedPackage, PackageObservation};

#[doc = "Persisted package lifecycle state."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleState {
    #[doc = "Package identity and `PackageManager` inode anchor are healthy."]
    Normal,
    #[doc = "`Gate` acquisition and base restoration are in progress."]
    UpdatePreparing,
    #[doc = "The package is gated on base while an installer may replace the `APK`."]
    UpdateWindowOpen,
    #[doc = "A completed replacement is being checked before gate release."]
    UpdateVerifying,
    #[doc = "A version, path, or inode changed outside a managed update window."]
    LifecycleDrifted,
    #[doc = "The package is gated while a user-approved base repair is pending."]
    RepairWaiting,
    #[doc = "The runtime cannot prove a safe data view and must keep the package disabled."]
    RecoveryRequired,
    #[doc = "Identity changed so old slots cannot be automatically attached."]
    Quarantined,
}

impl LifecycleState {
    #[doc = "Returns whether a durable state transition is legal."]
    pub const fn can_transition_to(self, next: Self) -> bool {
        match self {
            Self::Normal => matches!(
                next,
                Self::UpdatePreparing
                    | Self::LifecycleDrifted
                    | Self::RecoveryRequired
                    | Self::Quarantined
            ),
            Self::UpdatePreparing => {
                matches!(next, Self::UpdateWindowOpen | Self::RecoveryRequired)
            }
            Self::UpdateWindowOpen => {
                matches!(next, Self::UpdateVerifying | Self::RecoveryRequired)
            }
            Self::UpdateVerifying => matches!(
                next,
                Self::Normal | Self::RepairWaiting | Self::RecoveryRequired | Self::Quarantined
            ),
            Self::LifecycleDrifted => matches!(
                next,
                Self::RepairWaiting | Self::RecoveryRequired | Self::Quarantined
            ),
            Self::RepairWaiting => {
                matches!(
                    next,
                    Self::Normal | Self::RecoveryRequired | Self::Quarantined
                )
            }
            Self::RecoveryRequired => matches!(next, Self::RepairWaiting | Self::Quarantined),
            Self::Quarantined => false,
        }
    }
}

#[doc = "Fail-closed reason associated with a recovery-required decision."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryReason {
    #[doc = "A persisted transitional lifecycle state has no accepted update proof."]
    PersistedLifecycleState,
    #[doc = "`PackageManager` no longer points to the enrolled base inodes."]
    PackageManagerInodeDrift,
    #[doc = "Canonical and `App`-process views disagree with the committed slot."]
    VisibleViewDrift,
    #[doc = "Version or code path changed outside an update verification state."]
    UnexpectedPackageReplacement,
    #[doc = "The package was already persisted as recovery-required."]
    PersistedRecoveryState,
}

#[doc = "Package lifecycle decision made before any gate or mount mutation."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardDecision {
    #[doc = "Base may remain active."]
    AllowBase,
    #[doc = "A non-base slot may remain active while `PackageManager` stays anchored to base."]
    AllowSlot,
    #[doc = "The replacement is inside a managed update verification window."]
    AllowUpdateVerification,
    #[doc = "A pending installer requires gate acquisition and base restoration."]
    RequireSafeUpdateWindow,
    #[doc = "No safe automatic action may proceed."]
    RecoveryRequired(RecoveryReason),
    #[doc = "The package identity no longer owns the enrolled slots."]
    Quarantine,
}

#[doc = "Pure package lifecycle guard evaluated before each switch, launch, boot, and update action."]
#[derive(Debug)]
pub struct PackageLifecycleGuard;

impl PackageLifecycleGuard {
    #[doc = "Assesses a persisted package contract against one coherent live observation."]
    pub fn assess(managed: &ManagedPackage, observed: &PackageObservation) -> GuardDecision {
        Self::assess_with_accepted_artifact(managed, observed, None)
    }

    #[doc = "Assesses live facts using enrollment owner identity and the accepted package artifact."]
    #[doc = "A missing artifact is the `schema-v1` compatibility path and falls back to enrollment."]
    pub fn assess_with_accepted_artifact(
        managed: &ManagedPackage,
        observed: &PackageObservation,
        accepted_artifact: Option<&InstalledArtifact>,
    ) -> GuardDecision {
        let expected_owner = managed.identity().owner_identity();
        let actual = observed.identity();
        let lifecycle = managed.lifecycle_state();

        if lifecycle == LifecycleState::Quarantined
            || expected_owner.uid() != actual.uid()
            || expected_owner.signature_sha256() != actual.signature_sha256()
        {
            return GuardDecision::Quarantine;
        }
        if lifecycle == LifecycleState::RecoveryRequired {
            return GuardDecision::RecoveryRequired(RecoveryReason::PersistedRecoveryState);
        }
        if !matches!(
            lifecycle,
            LifecycleState::Normal | LifecycleState::UpdateVerifying
        ) {
            return GuardDecision::RecoveryRequired(RecoveryReason::PersistedLifecycleState);
        }
        if observed.package_manager_inodes() != managed.base_inodes() {
            return GuardDecision::RecoveryRequired(RecoveryReason::PackageManagerInodeDrift);
        }
        if observed.pending_install()
            && !matches!(
                lifecycle,
                LifecycleState::UpdateWindowOpen | LifecycleState::UpdateVerifying
            )
        {
            return GuardDecision::RequireSafeUpdateWindow;
        }

        let expected_version_code = accepted_artifact.map_or_else(
            || managed.identity().version_code(),
            InstalledArtifact::version_code,
        );
        let expected_code_path = accepted_artifact.map_or_else(
            || managed.identity().code_path(),
            InstalledArtifact::code_path,
        );
        let package_replaced = expected_version_code != actual.version_code()
            || expected_code_path != actual.code_path();
        if package_replaced {
            if lifecycle == LifecycleState::UpdateVerifying
                && managed.active_slot().is_base()
                && observed.canonical_inodes() == managed.base_inodes()
                && observed.active_process_inodes() == managed.base_inodes()
            {
                return GuardDecision::AllowUpdateVerification;
            }
            return GuardDecision::RecoveryRequired(RecoveryReason::UnexpectedPackageReplacement);
        }
        if lifecycle == LifecycleState::UpdateVerifying {
            return GuardDecision::RecoveryRequired(RecoveryReason::PersistedLifecycleState);
        }

        if observed.canonical_inodes() != managed.active_inodes()
            || observed.active_process_inodes() != managed.active_inodes()
        {
            return GuardDecision::RecoveryRequired(RecoveryReason::VisibleViewDrift);
        }

        if managed.active_slot().is_base() {
            GuardDecision::AllowBase
        } else {
            GuardDecision::AllowSlot
        }
    }
}
