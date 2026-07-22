use crate::android::PackageProbe;
use crate::domain::{GateSnapshot, ManagedPackage, PackageKey};
use crate::enrollment_attempt::{CommitProof, EnrollmentAttemptPhase, RetirementProof};
use crate::materializer::MaterializationBackend;
use crate::reconcile::RecoveryBackend;
use crate::service::{EnrollmentPublicationError, ServiceError};

use super::composition::ProductionPlatform;
use super::enrollment::{
    candidate_managed, capture_base, initial_managed, inspect_candidate, require_key,
};
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn do_begin_enrollment(
        &mut self,
        key: &PackageKey,
    ) -> Result<GateSnapshot, ServiceError> {
        require_key(key)?;
        if self
            .stores
            .attempts
            .load(key)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_some()
        {
            return Err(ServiceError::RecoveryRequired);
        }
        let snapshot = self
            .runtime
            .emergency_gate(key.package_name())
            .map_err(|_| ServiceError::RecoveryRequired)?;
        self.stores
            .attempts
            .create_pending(key, snapshot)
            .map_err(|_| ServiceError::RecoveryRequired)?;
        Ok(snapshot)
    }

    pub(super) fn do_hold_gate(&mut self, key: &PackageKey) -> Result<(), ServiceError> {
        let package = self.initial_package(key)?;
        if self.runtime.acquire_gate(&package).is_err() {
            self.runtime
                .confirm_emergency_gate(&package)
                .and_then(|()| self.runtime.acquire_gate(&package))
                .map_err(|_| ServiceError::RecoveryRequired)?;
        }
        self.runtime
            .verify_gate_held(&package)
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    pub(super) fn do_quiesce(&mut self, key: &PackageKey) -> Result<(), ServiceError> {
        let package = self.initial_package(key)?;
        self.runtime
            .confirm_emergency_gate(&package)
            .and_then(|()| self.runtime.quiesce_processes(&package))
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    pub(super) fn do_enroll(
        &mut self,
        key: &PackageKey,
        accept_direct_boot_conditional: bool,
    ) -> Result<ManagedPackage, EnrollmentPublicationError> {
        let (package, support_level) = {
            let mut probe = self
                .probe
                .try_borrow_mut()
                .map_err(|_| EnrollmentPublicationError::Unpublished(ServiceError::Busy))?;
            let package = initial_managed(&mut *probe, key)
                .map_err(EnrollmentPublicationError::Unpublished)?;
            let (candidate, compatibility) = inspect_candidate(&mut *probe, key)
                .map_err(EnrollmentPublicationError::Unpublished)?;
            let same_identity = candidate.identity() == package.identity();
            let same_inodes = candidate.base_inodes() == package.base_inodes();
            if !same_identity || !same_inodes {
                return Err(EnrollmentPublicationError::Unpublished(
                    ServiceError::RecoveryRequired,
                ));
            }
            (package, compatibility.support_level())
        };
        if support_level == crate::domain::PackageSupportLevel::DirectBootConditional
            && !accept_direct_boot_conditional
        {
            return Err(EnrollmentPublicationError::Unpublished(
                ServiceError::DirectBootConfirmationRequired,
            ));
        }
        let base = capture_base(&mut self.materializer, &package)
            .map_err(EnrollmentPublicationError::Unpublished)?;
        {
            let mut probe = self
                .probe
                .try_borrow_mut()
                .map_err(|_| EnrollmentPublicationError::Unpublished(ServiceError::Busy))?;
            let (final_package, final_compatibility) = inspect_candidate(&mut *probe, key)
                .map_err(EnrollmentPublicationError::Unpublished)?;
            if final_package.identity() != package.identity()
                || final_package.base_inodes() != package.base_inodes()
                || final_compatibility.support_level() != support_level
            {
                return Err(EnrollmentPublicationError::Unpublished(
                    ServiceError::RecoveryRequired,
                ));
            }
        }
        let published: Result<ManagedPackage, ServiceError> = (|| {
            self.stores
                .enrollment
                .create(&package)
                .map_err(|_| ServiceError::RecoveryRequired)?;
            self.stores
                .compatibility_policy
                .create(
                    key.package_name(),
                    package.identity(),
                    support_level,
                    support_level == crate::domain::PackageSupportLevel::DirectBootConditional,
                )
                .map_err(|_| ServiceError::RecoveryRequired)?;
            self.stores
                .catalog
                .create_base(
                    key.clone(),
                    package.base_inodes(),
                    package.identity().clone(),
                    base.security().clone(),
                )
                .map_err(|_| ServiceError::RecoveryRequired)?;
            let state = self
                .stores
                .package_state
                .initialize(key)
                .map_err(|_| ServiceError::RecoveryRequired)?;
            let digests = self.stores.published_digests(key, &state)?;
            let proof = CommitProof::new(
                package.clone(),
                &digests.enrollment,
                &digests.compatibility_policy,
                &digests.base_catalog,
                &digests.package_state,
            )
            .map_err(|_| ServiceError::RecoveryRequired)?;
            self.stores
                .attempts
                .commit_published(key, &proof)
                .map_err(|_| ServiceError::RecoveryRequired)?;
            Ok(package.clone())
        })();
        if published.is_err() {
            let _ = self.stores.attempts.mark_recovery_required(key);
        }
        published.map_err(|_| EnrollmentPublicationError::PublicationAmbiguous)
    }

    pub(super) fn do_abort_enrollment(
        &mut self,
        key: &PackageKey,
        snapshot: GateSnapshot,
    ) -> Result<(), ServiceError> {
        let result = (|| {
            let package = self.initial_package(key)?;
            self.runtime
                .confirm_emergency_gate(&package)
                .and_then(|()| self.runtime.restore_gate(&package, snapshot))
                .and_then(|()| self.runtime.retire_gate_lease(&package))
                .map_err(|_| ServiceError::RecoveryRequired)?;
            self.stores
                .attempts
                .abort_pending(key, RetirementProof::new(snapshot))
                .map_err(|_| ServiceError::RecoveryRequired)
        })();
        if result.is_err() {
            let _ = self.runtime.emergency_gate(key.package_name());
        }
        result
    }

    pub(super) fn do_retire_enrollment(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), ServiceError> {
        let key = PackageKey::new(package.package_name().clone(), package.user_id());
        let attempt = self
            .stores
            .attempts
            .load(&key)
            .map_err(|_| ServiceError::RecoveryRequired)?
            .ok_or(ServiceError::RecoveryRequired)?;
        if attempt.phase() != EnrollmentAttemptPhase::Committed || !attempt.is_authoritative() {
            return Err(ServiceError::RecoveryRequired);
        }
        let result = self
            .runtime
            .retire_gate_lease(package)
            .map_err(|_| ServiceError::RecoveryRequired)
            .and_then(|()| {
                self.stores
                    .attempts
                    .retire(&key, RetirementProof::new(attempt.gate_snapshot()))
                    .map_err(|_| ServiceError::RecoveryRequired)
            });
        if result.is_err() {
            let _ = self.runtime.emergency_gate(package.package_name());
        }
        result
    }

    fn initial_package(&self, key: &PackageKey) -> Result<ManagedPackage, ServiceError> {
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        candidate_managed(&mut *probe, key)
    }
}
