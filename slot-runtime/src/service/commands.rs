use crate::domain::PackageName;
use crate::protocol::{
    Ack, AckOperation, PackageSnapshotReport, PackageStatus, ProbeReport, ResponsePayload,
    SlotSummary, SlotsReport,
};

use super::outcome;
use super::validation;
use super::{
    EnrollmentPublicationError, PackageState, PreviewService, ServiceError, ServicePlatform,
};

impl<P: ServicePlatform> PreviewService<P> {
    pub(super) fn probe_command(&self) -> Result<ResponsePayload, ServiceError> {
        let capabilities = self.platform.probe()?;
        Ok(ResponsePayload::ProbeReport(ProbeReport::new(
            capabilities.ready(),
            capabilities.user_unlocked(),
            capabilities.ce_de_supported(),
            capabilities.recovery_only(),
        )))
    }

    pub(super) fn status_command(
        &self,
        package: &PackageName,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        let snapshot = validation::snapshot(&key, self.platform.package_state(&key)?)?;
        validation::reportable(snapshot.managed())?;
        let gate = snapshot.gate();
        Ok(ResponsePayload::PackageStatus(PackageStatus::new(
            package.clone(),
            snapshot.managed().active_slot().clone(),
            snapshot.managed().lifecycle_state(),
            gate.enabled(),
            gate.suspended(),
        )))
    }

    pub(super) fn package_snapshot_command(
        &self,
        package: &PackageName,
    ) -> Result<ResponsePayload, ServiceError> {
        let capability = self.platform.probe()?;
        if !capability.user_unlocked() {
            return Err(ServiceError::UserLocked);
        }
        if !capability.ready() || !capability.ce_de_supported() {
            return Err(ServiceError::UnsupportedDevice);
        }
        let key = validation::package_key(package);
        let snapshot = validation::snapshot(&key, self.platform.package_state(&key)?)?;
        validation::reportable(snapshot.managed())?;
        let gate = snapshot.gate();
        let status = PackageStatus::new(
            package.clone(),
            snapshot.managed().active_slot().clone(),
            snapshot.managed().lifecycle_state(),
            gate.enabled(),
            gate.suspended(),
        );
        let slots = self
            .platform
            .list_slots_for_snapshot(&key, &snapshot)?
            .into_iter()
            .map(SlotSummary::from)
            .collect();
        let report = PackageSnapshotReport::new(status, SlotsReport::new(package.clone(), slots))
            .map_err(|_| ServiceError::RecoveryRequired)?;
        Ok(ResponsePayload::PackageSnapshot(report))
    }

    pub(super) fn enroll_command(
        &mut self,
        package: &PackageName,
        accept_direct_boot_conditional: bool,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        match self.platform.package_state(&key)? {
            PackageState::Absent => {}
            state => {
                let snapshot = validation::snapshot(&key, state)?;
                validation::reportable(snapshot.managed())?;
                return Ok(enroll_ack());
            }
        }
        let inspection = self.platform.inspect_package(&key)?;
        match inspection.compatibility().support_level() {
            crate::domain::PackageSupportLevel::Supported => {}
            crate::domain::PackageSupportLevel::DirectBootConditional
                if accept_direct_boot_conditional => {}
            crate::domain::PackageSupportLevel::DirectBootConditional => {
                return Err(ServiceError::DirectBootConfirmationRequired);
            }
            crate::domain::PackageSupportLevel::Blocked => {
                return Err(ServiceError::PackageNotAllowed);
            }
        }
        let gate = self.platform.begin_enrollment_attempt(&key)?;
        if let Err(cause) = self.platform.hold_gate(&key) {
            return Err(self.abort_unpublished_enrollment(&key, gate, cause));
        }
        if let Err(cause) = self.platform.quiesce(&key) {
            return Err(self.abort_unpublished_enrollment(&key, gate, cause));
        }
        let managed = match self
            .platform
            .enroll_atomically(&key, accept_direct_boot_conditional)
        {
            Ok(managed) => managed,
            Err(EnrollmentPublicationError::Unpublished(cause)) => {
                return Err(self.abort_unpublished_enrollment(&key, gate, cause));
            }
            Err(EnrollmentPublicationError::PublicationAmbiguous) => {
                let class = ServiceError::RecoveryRequired;
                if self.contain_unenrolled(&key, class).is_err() {
                    return Err(ServiceError::RecoveryRequired);
                }
                return Err(class);
            }
        };
        if validation::enrolled(&key, &managed).is_err()
            || self.platform.prove_base(&managed).is_err()
            || self.platform.restore_gate(&managed, gate).is_err()
        {
            if self.contain_enrolled(&key, &managed).is_err() {
                return Err(ServiceError::RecoveryRequired);
            }
            return Err(ServiceError::RecoveryRequired);
        }
        if self.platform.retire_gate_lease(&managed).is_err() {
            self.contain_enrolled(&key, &managed)
                .map_err(|_| ServiceError::RecoveryRequired)?;
            return Err(ServiceError::RecoveryRequired);
        }
        Ok(enroll_ack())
    }

    pub(super) fn rescue_command(
        &mut self,
        package: &PackageName,
    ) -> Result<ResponsePayload, ServiceError> {
        let key = validation::package_key(package);
        outcome::rescued(self.platform.rescue_to_base(&key)?)
    }

    fn abort_unpublished_enrollment(
        &mut self,
        key: &crate::domain::PackageKey,
        gate: crate::domain::GateSnapshot,
        cause: ServiceError,
    ) -> ServiceError {
        match self.platform.abort_enrollment_attempt(key, gate) {
            Ok(()) => cause,
            Err(_) => ServiceError::RecoveryRequired,
        }
    }

    fn contain_unenrolled(
        &mut self,
        key: &crate::domain::PackageKey,
        class: ServiceError,
    ) -> Result<(), ServiceError> {
        self.platform
            .hold_gate(key)
            .map_err(ServiceError::after_gate)?;
        self.platform
            .quiesce(key)
            .map_err(ServiceError::after_gate)?;
        self.platform
            .mark_enrollment_failure(key, class)
            .map_err(ServiceError::after_gate)
    }

    fn contain_enrolled(
        &mut self,
        key: &crate::domain::PackageKey,
        managed: &crate::domain::ManagedPackage,
    ) -> Result<(), ServiceError> {
        self.platform
            .hold_gate(key)
            .map_err(ServiceError::after_gate)?;
        self.platform
            .quiesce(key)
            .map_err(ServiceError::after_gate)?;
        self.platform
            .mark_recovery_required(managed)
            .map_err(ServiceError::after_gate)
    }
}

const fn enroll_ack() -> ResponsePayload {
    ResponsePayload::Ack(Ack::new(AckOperation::EnrollPackage))
}
