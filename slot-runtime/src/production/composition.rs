use std::cell::RefCell;

use crate::android::PackageProbe;
use crate::domain::{GateSnapshot, ManagedPackage, PackageKey, SlotView};
use crate::materializer::MaterializationBackend;
use crate::reconcile::{NativeBaseRecoveryBackend, ReconcileOutcome, RecoveryBackend};
use crate::rescue::{RescueExecution, RescueJournalStore, RescueMetadataSource, RescueStartup};
use crate::service::{
    CapabilitySnapshot, EnrollmentPublicationError, PackageState, ServiceError, ServicePlatform,
    SwitchExecution,
};

use super::metadata::MetadataSource;
use super::stores::ProductionStores;

/// Fixed-layout composition of Android adapters and durable Preview stores.
#[derive(Debug)]
pub struct ProductionPlatform<B, M, Q, T> {
    pub(super) runtime: B,
    pub(super) materializer: M,
    pub(super) probe: RefCell<Q>,
    pub(super) metadata: T,
    pub(super) stores: ProductionStores,
}

impl<B, M, Q, T> ProductionPlatform<B, M, Q, T> {
    /// Opens only the compiled runtime layout around injected typed adapters.
    pub fn open_fixed(
        runtime: B,
        materializer: M,
        probe: Q,
        metadata: T,
    ) -> Result<Self, ServiceError> {
        Ok(Self {
            runtime,
            materializer,
            probe: RefCell::new(probe),
            metadata,
            stores: ProductionStores::open_fixed()?,
        })
    }

    /// Returns the runtime adapter for read-only composition diagnostics.
    pub const fn runtime(&self) -> &B {
        &self.runtime
    }

    /// Returns the materialization adapter for read-only composition diagnostics.
    pub const fn materializer(&self) -> &M {
        &self.materializer
    }
}

impl<B, M, Q, T> ServicePlatform for ProductionPlatform<B, M, Q, T>
where
    B: RecoveryBackend + NativeBaseRecoveryBackend,
    M: MaterializationBackend,
    Q: PackageProbe,
    T: MetadataSource + RescueMetadataSource,
{
    fn probe(&self, key: &PackageKey) -> Result<CapabilitySnapshot, ServiceError> {
        self.do_probe(key)
    }

    fn package_state(&self, key: &PackageKey) -> Result<PackageState, ServiceError> {
        let rescue = RescueJournalStore::fixed().map_err(|_| ServiceError::RecoveryRequired)?;
        if rescue
            .load()
            .map_err(|_| ServiceError::RecoveryRequired)?
            .is_some()
        {
            return Ok(PackageState::RecoveryRequired);
        }
        let mut probe = self
            .probe
            .try_borrow_mut()
            .map_err(|_| ServiceError::Busy)?;
        super::state::load(&self.stores, &mut *probe, key)
    }

    fn capture_gate(&mut self, key: &PackageKey) -> Result<GateSnapshot, ServiceError> {
        self.do_capture_gate(key)
    }

    fn begin_enrollment_attempt(&mut self, key: &PackageKey) -> Result<GateSnapshot, ServiceError> {
        self.do_begin_enrollment(key)
    }

    fn abort_enrollment_attempt(
        &mut self,
        key: &PackageKey,
        snapshot: GateSnapshot,
    ) -> Result<(), ServiceError> {
        self.do_abort_enrollment(key, snapshot)
    }

    fn hold_gate(&mut self, key: &PackageKey) -> Result<(), ServiceError> {
        self.do_hold_gate(key)
    }

    fn quiesce(&mut self, key: &PackageKey) -> Result<(), ServiceError> {
        self.do_quiesce(key)
    }

    fn enroll_atomically(
        &mut self,
        key: &PackageKey,
    ) -> Result<ManagedPackage, EnrollmentPublicationError> {
        self.do_enroll(key)
    }

    fn prove_base(&mut self, package: &ManagedPackage) -> Result<(), ServiceError> {
        self.runtime
            .verify_native_base(package)
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    fn restore_gate(
        &mut self,
        package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), ServiceError> {
        self.runtime
            .restore_gate(package, snapshot)
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    fn retire_gate_lease(&mut self, package: &ManagedPackage) -> Result<(), ServiceError> {
        self.do_retire_enrollment(package)
    }

    fn mark_recovery_required(&mut self, package: &ManagedPackage) -> Result<(), ServiceError> {
        self.mark_package_recovery(package)
    }

    fn mark_enrollment_failure(
        &mut self,
        key: &PackageKey,
        _class: ServiceError,
    ) -> Result<(), ServiceError> {
        self.stores
            .attempts
            .mark_recovery_required(key)
            .map(|_| ())
            .map_err(|_| ServiceError::RecoveryRequired)
    }

    fn contain_failure(
        &mut self,
        package: &ManagedPackage,
        class: ServiceError,
    ) -> Result<(), ServiceError> {
        self.do_contain(package, class)
    }

    fn materialize_preview(&mut self, package: &ManagedPackage) -> Result<SlotView, ServiceError> {
        self.do_materialize(package)
    }

    fn switch_view(
        &mut self,
        package: &ManagedPackage,
        target: &SlotView,
        prepared_gate: Option<GateSnapshot>,
    ) -> Result<SwitchExecution, ServiceError> {
        self.do_switch(package, target, prepared_gate)
    }

    fn reconcile_two_phase(&mut self, key: &PackageKey) -> Result<ReconcileOutcome, ServiceError> {
        match self.do_rescue_startup(key) {
            RescueStartup::OpenOrdinary => self.do_reconcile(key),
            RescueStartup::BaseRetired => Ok(ReconcileOutcome::RestoredBase),
            RescueStartup::RecoveryRequired => Ok(ReconcileOutcome::RecoveryRequired(
                crate::reconcile::ReconcileReason::JournalMetadata,
            )),
            RescueStartup::Quarantined => Ok(ReconcileOutcome::Quarantined),
            RescueStartup::ContainmentFailed => Err(ServiceError::Internal),
        }
    }

    fn rescue_to_base(&mut self, key: &PackageKey) -> Result<RescueExecution, ServiceError> {
        Ok(self.do_rescue_to_base(key))
    }
}
