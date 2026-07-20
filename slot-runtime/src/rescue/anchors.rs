use std::path::Path;

use crate::catalog::CatalogStore;
use crate::domain::{ManagedPackage, PackageKey};
use crate::enrollment::EnrollmentStore;
use crate::lifecycle::LifecycleState;
use crate::reconcile::RecoveryBackend;

use super::{RescueError, RescueSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RescueAnchors {
    base: ManagedPackage,
    enrollment_sha256: String,
    base_manifest_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LiveDecision {
    Valid,
    RecoveryRequired,
    Quarantined,
}

impl RescueAnchors {
    pub(super) fn load(
        enrollment_root: &Path,
        catalog_root: &Path,
        key: &PackageKey,
    ) -> Result<Self, RescueError> {
        let enrollment = EnrollmentStore::load_exact_secure(enrollment_root, key.package_name())
            .map_err(|error| RescueError::Corrupt(error.to_string()))?;
        let base = CatalogStore::load_exact_base_secure(catalog_root, key)
            .map_err(|error| RescueError::Corrupt(error.to_string()))?;
        let managed = enrollment.managed();
        let entry = base.entry();
        if managed.package_name() != key.package_name()
            || managed.user_id() != key.user_id()
            || !managed.active_slot().is_base()
            || managed.active_inodes() != managed.base_inodes()
            || managed.lifecycle_state() != LifecycleState::Normal
            || entry.package_key() != key
            || !entry.slot_id().is_base()
            || entry.inodes() != managed.base_inodes()
            || entry.enrolled_identity() != managed.identity()
        {
            return Err(RescueError::Corrupt(
                "base anchors do not form one immutable enrollment".to_owned(),
            ));
        }
        Ok(Self {
            base: managed.clone(),
            enrollment_sha256: enrollment.sha256().to_owned(),
            base_manifest_sha256: base.sha256().to_owned(),
        })
    }

    pub(super) const fn base(&self) -> &ManagedPackage {
        &self.base
    }

    pub(super) fn matches_spec(&self, spec: &RescueSpec) -> bool {
        spec.package_key().package_name() == self.base.package_name()
            && spec.package_key().user_id() == self.base.user_id()
            && spec.enrolled_identity() == self.base.identity()
            && spec.base_inodes() == self.base.base_inodes()
            && spec.enrollment_sha256() == self.enrollment_sha256
            && spec.base_manifest_sha256() == self.base_manifest_sha256
    }

    pub(super) fn spec_digests(&self) -> (&str, &str) {
        (&self.enrollment_sha256, &self.base_manifest_sha256)
    }

    pub(super) fn validate_live<B: RecoveryBackend>(&self, backend: &mut B) -> LiveDecision {
        let Ok(unlocked) = backend.user0_unlocked() else {
            return LiveDecision::RecoveryRequired;
        };
        if !unlocked {
            return LiveDecision::RecoveryRequired;
        }
        let Ok(observed) = backend.observe_package(&self.base) else {
            return LiveDecision::RecoveryRequired;
        };
        let enrolled = self.base.identity();
        let live = observed.identity();
        if live.uid() != enrolled.uid() || live.signature_sha256() != enrolled.signature_sha256() {
            return LiveDecision::Quarantined;
        }
        if live.version_code() != enrolled.version_code()
            || live.code_path() != enrolled.code_path()
            || observed.package_manager_inodes() != self.base.base_inodes()
            || observed.pending_install()
        {
            return LiveDecision::RecoveryRequired;
        }
        LiveDecision::Valid
    }
}
