use crate::android::PackageProbe;
use crate::domain::{GateSnapshot, ManagedPackage, PackageKey};
use crate::enrollment_attempt::{CommitProof, EnrollmentAttemptPhase, RetirementProof};
use crate::materializer::MaterializationBackend;
use crate::reconcile::RecoveryBackend;
use crate::service::{CapabilitySnapshot, EnrollmentPublicationError, PackageState, ServiceError};

use super::composition::ProductionPlatform;
use super::enrollment::{capture_base, initial_managed, require_key};
use super::metadata::MetadataSource;

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource,
{
    pub(super) fn do_probe(&self, key: &PackageKey) -> Result<CapabilitySnapshot, ServiceError> {
        require_key(key)?;
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        let unlocked = probe
            .user0_unlocked()
            .map_err(|_| ServiceError::UnsupportedDevice)?;
        let global = probe
            .mount_namespace_proof()
            .map_err(|_| ServiceError::UnsupportedDevice)?
            .is_global();
        let paired = probe
            .view_proof(key.package_name(), key.user_id())
            .is_ok_and(|proof| {
                proof.canonical().inodes().ce() != proof.canonical().inodes().de()
                    && proof.mirror_inodes().ce() != proof.mirror_inodes().de()
                    && proof.zygote_inodes().ce() != proof.zygote_inodes().de()
            });
        Ok(CapabilitySnapshot::new(
            unlocked && global && paired,
            unlocked,
            paired,
        ))
    }

    pub(super) fn do_capture_gate(
        &mut self,
        key: &PackageKey,
    ) -> Result<GateSnapshot, ServiceError> {
        let package = self.ready_managed(key)?;
        self.runtime
            .capture_gate_snapshot(&package)
            .map_err(|_| ServiceError::Internal)
    }

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
    ) -> Result<ManagedPackage, EnrollmentPublicationError> {
        let package = self
            .initial_package(key)
            .map_err(EnrollmentPublicationError::Unpublished)?;
        let base = capture_base(&mut self.materializer, &package)
            .map_err(EnrollmentPublicationError::Unpublished)?;
        let published: Result<ManagedPackage, ServiceError> = (|| {
            self.stores
                .enrollment
                .create(&package)
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
        initial_managed(&mut *probe, key)
    }

    fn ready_managed(&self, key: &PackageKey) -> Result<ManagedPackage, ServiceError> {
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        match super::state::load(&self.stores, &mut *probe, key)? {
            PackageState::Ready(snapshot) => Ok(snapshot.managed().clone()),
            PackageState::Absent => Err(ServiceError::NotFound),
            PackageState::RecoveryRequired => Err(ServiceError::RecoveryRequired),
            PackageState::Quarantined => Err(ServiceError::Quarantined),
        }
    }
}
