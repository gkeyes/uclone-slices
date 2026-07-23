use crate::domain::{GateSnapshot, ManagedPackage, PackageKey, SlotView};
use crate::launch::LaunchDisposition;
use crate::reconcile::ReconcileOutcome;
use crate::rescue::RescueExecution;

use super::{
    CapabilitySnapshot, EnrollmentPublicationError, PackageSnapshot, PackageState, ServiceError,
    SwitchExecution,
};

/// Minimal injected platform composition required by [`super::PreviewService`].
///
/// Implementations may compose the existing enrollment, catalog, package-state,
/// materializer, runtime, and reconciliation coordinators. No method accepts a
/// caller-supplied filesystem path or Android user id.
pub trait ServicePlatform: core::fmt::Debug {
    /// Reads device and user-zero capability facts without mutating runtime state.
    fn probe(&self) -> Result<CapabilitySnapshot, ServiceError>;

    /// Inspects one installed user-zero package without enrolling it.
    fn inspect_package(&self, _key: &PackageKey) -> Result<super::PackageInspection, ServiceError> {
        Err(ServiceError::InvalidRequest)
    }

    /// Lists durable managed-app rows without accepting caller paths.
    fn list_managed_apps(&self) -> Result<Vec<super::ManagedAppInfo>, ServiceError> {
        Err(ServiceError::InvalidRequest)
    }

    /// Lists fixed-root user-zero packages accepted by recovery-only rescue.
    fn list_recovery_targets(&self) -> Result<Vec<crate::domain::PackageName>, ServiceError> {
        Err(ServiceError::InvalidRequest)
    }

    /// Lists Base and verified non-base slots for one managed package.
    fn list_slots(&self, _key: &PackageKey) -> Result<Vec<super::SlotInfo>, ServiceError> {
        Err(ServiceError::InvalidRequest)
    }

    /// Lists slots using the exact package snapshot already validated by the caller.
    fn list_slots_for_snapshot(
        &self,
        key: &PackageKey,
        _snapshot: &PackageSnapshot,
    ) -> Result<Vec<super::SlotInfo>, ServiceError> {
        self.list_slots(key)
    }

    /// Creates a runtime-named slot, materializes it, and switches transactionally.
    fn create_slot(
        &mut self,
        _key: &PackageKey,
        _display_name: crate::slot_metadata::SlotDisplayName,
        _seed_mode: crate::slot_metadata::SlotSeedMode,
    ) -> Result<SwitchExecution, ServiceError> {
        Err(ServiceError::InvalidRequest)
    }

    /// Appends a display-only rename revision for one ready non-base slot.
    fn rename_slot(
        &mut self,
        _key: &PackageKey,
        _slot: &crate::domain::SlotId,
        _display_name: crate::slot_metadata::SlotDisplayName,
    ) -> Result<(), ServiceError> {
        Err(ServiceError::InvalidRequest)
    }

    /// Deletes paired storage for one inactive non-base slot and tombstones it.
    fn delete_slot(
        &mut self,
        _key: &PackageKey,
        _slot: &crate::domain::SlotId,
    ) -> Result<(), ServiceError> {
        Err(ServiceError::InvalidRequest)
    }

    /// Reads and cross-validates all package stores without mutating them.
    fn package_state(&self, key: &PackageKey) -> Result<PackageState, ServiceError>;

    /// Captures and durably leases the exact pre-gate package state.
    fn capture_gate(&mut self, key: &PackageKey) -> Result<GateSnapshot, ServiceError>;

    /// Captures the exact pre-enrollment gate and publishes a root-level attempt
    /// anchor that reconciliation can enumerate without an enrollment record.
    fn begin_enrollment_attempt(&mut self, key: &PackageKey) -> Result<GateSnapshot, ServiceError>;

    /// Exact-restores and retires an attempt anchor before enrollment publication
    /// begins. Failure must leave the anchor enumerable for reboot reconciliation.
    fn abort_enrollment_attempt(
        &mut self,
        key: &PackageKey,
        snapshot: GateSnapshot,
    ) -> Result<(), ServiceError>;

    /// Acquires and proves the package execution gate for user zero.
    fn hold_gate(&mut self, key: &PackageKey) -> Result<(), ServiceError>;

    /// Stops and proves the absence of every process owned by the package.
    fn quiesce(&mut self, key: &PackageKey) -> Result<(), ServiceError>;

    /// Captures identity, base CE/DE inodes, and security metadata, then publishes
    /// enrollment, base catalog, and initial package state. Failures explicitly
    /// distinguish a proved-unpublished attempt from ambiguous partial publication.
    fn enroll_atomically(
        &mut self,
        key: &PackageKey,
        accept_direct_boot_conditional: bool,
    ) -> Result<ManagedPackage, EnrollmentPublicationError>;

    /// Proves the native immutable base view before an exact gate restoration.
    fn prove_base(&mut self, package: &ManagedPackage) -> Result<(), ServiceError>;

    /// Restores exactly the state returned by [`Self::capture_gate`].
    fn restore_gate(
        &mut self,
        package: &ManagedPackage,
        snapshot: GateSnapshot,
    ) -> Result<(), ServiceError>;

    /// Retires the enrollment gate lease only after exact restoration was proved.
    fn retire_gate_lease(&mut self, package: &ManagedPackage) -> Result<(), ServiceError>;

    /// Durably maps an already-contained ambiguous result to package recovery state.
    fn mark_recovery_required(&mut self, package: &ManagedPackage) -> Result<(), ServiceError>;

    /// Retains and classifies a root-level enrollment-attempt recovery anchor when
    /// no immutable enrollment exists for ordinary reconciliation discovery.
    fn mark_enrollment_failure(
        &mut self,
        key: &PackageKey,
        class: ServiceError,
    ) -> Result<(), ServiceError>;

    /// Holds and quiesces after a façade-detected proof mismatch, then durably
    /// maps the supplied recovery or quarantine class to package state.
    fn contain_failure(
        &mut self,
        package: &ManagedPackage,
        class: ServiceError,
    ) -> Result<(), ServiceError>;

    /// Materializes and catalogs both CE and DE for the fixed `preview` slot while
    /// retaining the already-proved execution gate.
    fn materialize_slot(
        &mut self,
        package: &ManagedPackage,
        slot: &crate::domain::SlotId,
        seed_mode: crate::slot_metadata::SlotSeedMode,
    ) -> Result<SlotView, ServiceError>;

    /// Delegates to the runtime coordinator using only a typed catalog view.
    /// When `prepared_gate` is present, the coordinator must reuse that exact
    /// pre-materialization lease. Success requires view proof, exact gate restore,
    /// durable `GateReleased` then `Completed`, and lease retirement in that order.
    /// Retirement failure must re-hold and quiesce the package, map package state
    /// to recovery-required, and return [`SwitchExecution::RecoveryRequired`].
    fn switch_view(
        &mut self,
        package: &ManagedPackage,
        target: &SlotView,
        prepared_gate: Option<GateSnapshot>,
    ) -> Result<SwitchExecution, ServiceError>;

    /// Gates and quiesces the package, then proves the complete current view.
    fn verify_current_view_for_launch(
        &mut self,
        _package: &ManagedPackage,
        _target: &SlotView,
    ) -> Result<(), ServiceError> {
        Err(ServiceError::RecoveryRequired)
    }

    /// Opens the validated package's system-resolved user-zero launcher entry.
    ///
    /// The default is fail-closed so fake or recovery-only compositions cannot
    /// accidentally claim that an app was opened.
    fn launch_package(&mut self, _package: &ManagedPackage) -> LaunchDisposition {
        LaunchDisposition::Failed
    }

    /// Runs early-boot hold followed by user-unlocked two-phase reconciliation,
    /// including root-level enrollment-attempt anchors that have no enrollment.
    /// Every recovery or quarantine result must be durably mapped to package state;
    /// lease retirement follows exact restore and durable Journal completion only.
    fn reconcile_two_phase(&mut self, key: &PackageKey) -> Result<ReconcileOutcome, ServiceError>;

    /// Executes the independent native-base rescue from only an allowlisted key.
    /// Implementations must not require ordinary package state, Registry, Journal,
    /// catalog enumeration, or materializer state to be readable.
    fn rescue_to_base(&mut self, key: &PackageKey) -> Result<RescueExecution, ServiceError>;
}
