use thiserror::Error;

use crate::model::{
    Capabilities, DisplayName, ModelError, ObservedView, PackageAggregate, PackageInspection,
    PackageName, PackageSnapshot, SeedMode, SlotId,
};
use crate::ports::{AdapterError, AndroidOps, PackageStore, SlotStorage};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("invalid request")]
    InvalidRequest,
    #[error("package was not found")]
    NotFound,
    #[error("package state conflicts with the requested operation")]
    StateConflict,
    #[error("runtime operation failed")]
    OperationFailed,
}

#[derive(Debug)]
pub struct Runtime<P, S, A> {
    packages: P,
    slots: S,
    android: A,
}

impl<P, S, A> Runtime<P, S, A>
where
    P: PackageStore,
    S: SlotStorage,
    A: AndroidOps,
{
    pub fn new(packages: P, slots: S, android: A) -> Self {
        Self {
            packages,
            slots,
            android,
        }
    }

    pub fn probe(&mut self) -> Result<Capabilities, RuntimeError> {
        self.android.probe().map_err(adapter_error)
    }

    pub fn list_packages(&mut self) -> Result<Vec<PackageSnapshot>, RuntimeError> {
        let names = self.packages.list().map_err(adapter_error)?;
        let mut snapshots = Vec::with_capacity(names.len());
        for package in names {
            match self.get_package(&package) {
                Ok(snapshot) => snapshots.push(snapshot),
                Err(error) => {
                    eprintln!("op=list_packages package={package} step=load_package error={error}");
                }
            }
        }
        Ok(snapshots)
    }

    pub fn get_package(&mut self, package: &PackageName) -> Result<PackageSnapshot, RuntimeError> {
        let (aggregate, _inspection) = self.load_ready(package)?;
        aggregate.snapshot().map_err(model_error)
    }

    pub fn enroll(&mut self, package: PackageName) -> Result<PackageSnapshot, RuntimeError> {
        if self
            .packages
            .load(&package)
            .map_err(adapter_error)?
            .is_some()
        {
            return self.get_package(&package);
        }
        let inspection = self
            .android
            .inspect(&package)
            .map_err(|error| logged_adapter("enroll", &package, "inspect", error))?;
        let observed = self
            .android
            .observe_view(&package)
            .map_err(|error| logged_adapter("enroll", &package, "observe_base", error))?;
        if observed != ObservedView::Base {
            return Err(RuntimeError::StateConflict);
        }
        let aggregate = PackageAggregate::enrolled(package.clone(), inspection.identity);
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("enroll", &package, "save", error))?;
        aggregate.snapshot().map_err(model_error)
    }

    pub fn create_slot(
        &mut self,
        package: &PackageName,
        display_name: DisplayName,
        seed: SeedMode,
    ) -> Result<PackageSnapshot, RuntimeError> {
        let (mut aggregate, inspection) = self.load_ready(package)?;
        let observed = self
            .android
            .observe_view(package)
            .map_err(|error| logged_adapter("create_slot", package, "observe_active", error))?;
        if !observed.matches(aggregate.active_slot())
            || (seed == SeedMode::CloneBase && observed != ObservedView::Base)
        {
            return Err(RuntimeError::StateConflict);
        }

        let slot = aggregate.reserve_slot(display_name).map_err(model_error)?;
        let target = slot.id().clone();
        aggregate
            .begin_creation(&slot, inspection.was_running)
            .map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("create_slot", package, "save_intent", error))?;

        if let Err(error) = self.slots.discard(package, &target) {
            log_adapter("create_slot", package, "discard_stale_target", &error);
            self.rollback_creation(&mut aggregate, &target, false)?;
            return Err(RuntimeError::OperationFailed);
        }
        if let Err(error) = self.android.force_stop(package) {
            log_adapter("create_slot", package, "force_stop", &error);
            self.rollback_creation(&mut aggregate, &target, inspection.was_running)?;
            return Err(RuntimeError::OperationFailed);
        }
        if let Err(error) = self.slots.materialize(package, &slot, seed) {
            log_adapter("create_slot", package, "materialize_pair", &error);
            self.rollback_creation(&mut aggregate, &target, inspection.was_running)?;
            return Err(RuntimeError::OperationFailed);
        }
        if let Err(error) = self.slots.require_complete_pair(package, &target) {
            log_adapter("create_slot", package, "verify_pair", &error);
            self.rollback_creation(&mut aggregate, &target, inspection.was_running)?;
            return Err(RuntimeError::OperationFailed);
        }

        aggregate.finish_creation(slot).map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("create_slot", package, "save_commit", error))?;
        if inspection.was_running {
            self.android
                .launch_verified(package, aggregate.active_slot())
                .map_err(|error| logged_adapter("create_slot", package, "restore_launch", error))?;
        }
        aggregate.snapshot().map_err(model_error)
    }

    pub fn activate_slot(
        &mut self,
        package: &PackageName,
        target: &SlotId,
    ) -> Result<PackageSnapshot, RuntimeError> {
        let (mut aggregate, inspection) = self.load_ready(package)?;
        if !aggregate.has_slot(target) {
            return Err(RuntimeError::NotFound);
        }
        if !target.is_base() {
            self.slots
                .require_complete_pair(package, target)
                .map_err(|error| {
                    logged_conflict("activate_slot", package, "verify_target", error)
                })?;
        }
        let previous = aggregate.active_slot().clone();
        let observed = self
            .android
            .observe_view(package)
            .map_err(|error| logged_adapter("activate_slot", package, "observe_previous", error))?;
        if !observed.matches(&previous) {
            return Err(RuntimeError::StateConflict);
        }
        if &previous == target {
            self.android
                .launch_verified(package, target)
                .map_err(|error| logged_adapter("activate_slot", package, "retry_launch", error))?;
            return aggregate.snapshot().map_err(model_error);
        }

        aggregate.begin_activation(target).map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("activate_slot", package, "save_intent", error))?;
        if let Err(error) = self.android.force_stop(package) {
            log_adapter("activate_slot", package, "force_stop", &error);
            self.restore_after_failed_stop(&mut aggregate, &previous, inspection.was_running)?;
            return Err(RuntimeError::OperationFailed);
        }
        if let Err(error) = self.android.apply_view(package, target) {
            log_adapter("activate_slot", package, "apply_target", &error);
            self.rollback_activation(&mut aggregate, &previous, inspection.was_running)?;
            return Err(RuntimeError::OperationFailed);
        }
        let observed = self
            .android
            .observe_view(package)
            .map_err(|error| logged_adapter("activate_slot", package, "verify_target", error));
        if !matches!(observed, Ok(view) if view.matches(target)) {
            self.rollback_activation(&mut aggregate, &previous, inspection.was_running)?;
            return Err(RuntimeError::OperationFailed);
        }

        aggregate.finish_activation(target).map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("activate_slot", package, "save_commit", error))?;
        self.android
            .launch_verified(package, target)
            .map_err(|error| logged_adapter("activate_slot", package, "launch_target", error))?;
        aggregate.snapshot().map_err(model_error)
    }

    #[cfg(test)]
    fn into_parts(self) -> (P, S, A) {
        (self.packages, self.slots, self.android)
    }

    fn load_ready(
        &mut self,
        package: &PackageName,
    ) -> Result<(PackageAggregate, PackageInspection), RuntimeError> {
        let (mut aggregate, inspection) = self.load_validated(package)?;
        let recovering_activation = aggregate.pending_activation().is_some();
        self.recover(&mut aggregate)?;
        if recovering_activation {
            return Err(RuntimeError::StateConflict);
        }
        self.converge_ready_view(package, &aggregate, &inspection)?;
        Ok((aggregate, inspection))
    }

    fn load_validated(
        &mut self,
        package: &PackageName,
    ) -> Result<(PackageAggregate, PackageInspection), RuntimeError> {
        let aggregate = self
            .packages
            .load(package)
            .map_err(adapter_error)?
            .ok_or(RuntimeError::NotFound)?;
        aggregate.validate().map_err(|error| {
            eprintln!("op=load package={package} step=validate_persisted error={error}");
            RuntimeError::StateConflict
        })?;
        let inspection = self
            .android
            .inspect(package)
            .map_err(|error| logged_adapter("load", package, "inspect", error))?;
        if aggregate.identity() != &inspection.identity {
            eprintln!(
                "op=load package={package} step=verify_identity error=package_identity_changed"
            );
            return Err(RuntimeError::StateConflict);
        }
        Ok((aggregate, inspection))
    }

    fn converge_ready_view(
        &mut self,
        package: &PackageName,
        aggregate: &PackageAggregate,
        inspection: &PackageInspection,
    ) -> Result<(), RuntimeError> {
        let observed = self
            .android
            .observe_view(package)
            .map_err(|error| logged_adapter("load", package, "observe_ready", error))?;
        if !aggregate.is_ready() {
            return Err(RuntimeError::StateConflict);
        }
        if !observed.matches(aggregate.active_slot()) {
            if observed != ObservedView::Base || aggregate.active_slot().is_base() {
                return Err(RuntimeError::StateConflict);
            }
            self.slots
                .require_complete_pair(package, aggregate.active_slot())
                .map_err(|error| logged_conflict("load", package, "verify_ready_slot", error))?;
            self.android.force_stop(package).map_err(|error| {
                logged_conflict("load", package, "stop_for_ready_restore", error)
            })?;
            self.android
                .apply_view(package, aggregate.active_slot())
                .map_err(|error| logged_conflict("load", package, "apply_ready_restore", error))?;
            let restored = self
                .android
                .observe_view(package)
                .map_err(|error| logged_conflict("load", package, "verify_ready_restore", error))?;
            if !restored.matches(aggregate.active_slot()) {
                eprintln!(
                    "op=load package={package} step=verify_ready_restore error=view_not_restored"
                );
                return Err(RuntimeError::StateConflict);
            }
            if inspection.was_running {
                self.android
                    .launch_verified(package, aggregate.active_slot())
                    .map_err(|error| {
                        logged_conflict("load", package, "launch_ready_restore", error)
                    })?;
            }
        }
        Ok(())
    }

    fn recover(&mut self, aggregate: &mut PackageAggregate) -> Result<(), RuntimeError> {
        if let Some((target, restore_running)) = aggregate
            .pending_creation()
            .map(|(target, restore)| (target.clone(), restore))
        {
            let package = aggregate.package().clone();
            self.slots
                .discard(&package, &target)
                .map_err(|error| logged_conflict("recover", &package, "discard_creating", error))?;
            let observed = self
                .android
                .observe_view(&package)
                .map_err(|error| logged_conflict("recover", &package, "observe_creating", error))?;
            if !observed.matches(aggregate.active_slot()) {
                return Err(RuntimeError::StateConflict);
            }
            if restore_running {
                self.android
                    .launch_verified(&package, aggregate.active_slot())
                    .map_err(|error| {
                        logged_conflict("recover", &package, "restore_creating", error)
                    })?;
            }
            aggregate.abort_creation(&target).map_err(model_error)?;
            self.packages
                .save(aggregate)
                .map_err(|error| logged_conflict("recover", &package, "save_creating", error))?;
        }

        if let Some((previous, target)) = aggregate
            .pending_activation()
            .map(|(previous, target)| (previous.clone(), target.clone()))
        {
            let package = aggregate.package().clone();
            let observed = self.android.observe_view(&package).map_err(|error| {
                logged_conflict("recover", &package, "observe_activating", error)
            })?;
            if observed.matches(&target) {
                self.android
                    .launch_verified(&package, &target)
                    .map_err(|error| {
                        logged_conflict("recover", &package, "launch_target", error)
                    })?;
                aggregate.finish_activation(&target).map_err(model_error)?;
            } else {
                if !observed.matches(&previous) {
                    self.android.force_stop(&package).map_err(|error| {
                        logged_conflict("recover", &package, "force_stop_rollback", error)
                    })?;
                    self.android
                        .apply_view(&package, &previous)
                        .map_err(|error| {
                            logged_conflict("recover", &package, "apply_previous", error)
                        })?;
                    let rolled_back = self.android.observe_view(&package).map_err(|error| {
                        logged_conflict("recover", &package, "verify_previous", error)
                    })?;
                    if !rolled_back.matches(&previous) {
                        return Err(RuntimeError::StateConflict);
                    }
                }
                self.android
                    .launch_verified(&package, &previous)
                    .map_err(|error| {
                        logged_conflict("recover", &package, "launch_previous", error)
                    })?;
                aggregate.abort_activation(&previous).map_err(model_error)?;
            }
            self.packages
                .save(aggregate)
                .map_err(|error| logged_conflict("recover", &package, "save_activating", error))?;
        }

        Ok(())
    }

    fn rollback_creation(
        &mut self,
        aggregate: &mut PackageAggregate,
        target: &SlotId,
        restore_running: bool,
    ) -> Result<(), RuntimeError> {
        let package = aggregate.package().clone();
        self.slots
            .discard(&package, target)
            .map_err(|error| logged_adapter("create_slot", &package, "rollback_discard", error))?;
        aggregate.abort_creation(target).map_err(model_error)?;
        self.packages
            .save(aggregate)
            .map_err(|error| logged_adapter("create_slot", &package, "rollback_save", error))?;
        if restore_running {
            self.android
                .launch_verified(&package, aggregate.active_slot())
                .map_err(|error| {
                    logged_adapter("create_slot", &package, "rollback_launch", error)
                })?;
        }
        Ok(())
    }

    fn rollback_activation(
        &mut self,
        aggregate: &mut PackageAggregate,
        previous: &SlotId,
        restore_running: bool,
    ) -> Result<(), RuntimeError> {
        let package = aggregate.package().clone();
        if let Err(error) = self.android.apply_view(&package, previous) {
            log_adapter("activate_slot", &package, "rollback_apply", &error);
            return Err(RuntimeError::OperationFailed);
        }
        let observed = self
            .android
            .observe_view(&package)
            .map_err(|error| logged_adapter("activate_slot", &package, "rollback_verify", error))?;
        if !observed.matches(previous) {
            return Err(RuntimeError::OperationFailed);
        }
        aggregate.abort_activation(previous).map_err(model_error)?;
        self.packages
            .save(aggregate)
            .map_err(|error| logged_adapter("activate_slot", &package, "rollback_save", error))?;
        if restore_running {
            self.android
                .launch_verified(&package, previous)
                .map_err(|error| {
                    logged_adapter("activate_slot", &package, "rollback_launch", error)
                })?;
        }
        Ok(())
    }

    fn restore_after_failed_stop(
        &mut self,
        aggregate: &mut PackageAggregate,
        previous: &SlotId,
        restore_running: bool,
    ) -> Result<(), RuntimeError> {
        let package = aggregate.package().clone();
        let observed = self.android.observe_view(&package).map_err(|error| {
            logged_adapter("activate_slot", &package, "failed_stop_observe", error)
        })?;
        if !observed.matches(previous) {
            return Err(RuntimeError::OperationFailed);
        }
        if restore_running {
            self.android
                .launch_verified(&package, previous)
                .map_err(|error| {
                    logged_adapter("activate_slot", &package, "failed_stop_launch", error)
                })?;
        }
        aggregate.abort_activation(previous).map_err(model_error)?;
        self.packages
            .save(aggregate)
            .map_err(|error| logged_adapter("activate_slot", &package, "failed_stop_save", error))
    }
}

fn model_error(error: ModelError) -> RuntimeError {
    match error {
        ModelError::InvalidPackage | ModelError::InvalidSlot | ModelError::InvalidDisplayName => {
            RuntimeError::InvalidRequest
        }
        ModelError::SlotNotFound => RuntimeError::NotFound,
        ModelError::InvalidState | ModelError::StateConflict => RuntimeError::StateConflict,
    }
}

fn adapter_error(error: AdapterError) -> RuntimeError {
    if error.is_state_conflict() {
        RuntimeError::StateConflict
    } else {
        RuntimeError::OperationFailed
    }
}

fn logged_adapter(
    operation: &str,
    package: &PackageName,
    step: &str,
    error: AdapterError,
) -> RuntimeError {
    log_adapter(operation, package, step, &error);
    adapter_error(error)
}

fn logged_conflict(
    operation: &str,
    package: &PackageName,
    step: &str,
    error: AdapterError,
) -> RuntimeError {
    log_adapter(operation, package, step, &error);
    RuntimeError::StateConflict
}

fn log_adapter(operation: &str, package: &PackageName, step: &str, error: &AdapterError) {
    eprintln!("op={operation} package={package} step={step} error={error}");
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::adapters::{
        AndroidCall, AndroidFailure, FilePackageStore, MemoryAndroidOps, MemoryPackageStore,
        MemorySlotStorage, SlotFailure,
    };

    type TestRuntime = Runtime<MemoryPackageStore, MemorySlotStorage, MemoryAndroidOps>;

    fn package() -> PackageName {
        PackageName::new("com.example.app").unwrap()
    }

    fn runtime() -> TestRuntime {
        let mut android = MemoryAndroidOps::default();
        android.install(package());
        Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        )
    }

    fn with_slot() -> (TestRuntime, SlotId) {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone()).unwrap();
        let snapshot = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let target = snapshot.slots[1].id.clone();
        (runtime, target)
    }

    #[test]
    fn enroll_create_activate_is_one_runtime_flow() {
        let package = package();
        let (mut runtime, target) = with_slot();

        let activated = runtime.activate_slot(&package, &target).unwrap();

        assert_eq!(activated.active_slot, target);
        let (_packages, slots, android) = runtime.into_parts();
        assert!(slots.contains(&package, &activated.active_slot));
        assert_eq!(
            android.view(&package),
            Some(&ObservedView::Slot(activated.active_slot))
        );
        assert!(android.is_running(&package));
    }

    #[test]
    fn listing_skips_a_failed_package_and_keeps_healthy_packages() {
        let failed = PackageName::new("com.example.failed").unwrap();
        let healthy = PackageName::new("com.example.healthy").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(failed.clone());
        android.install(healthy.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        );
        runtime.enroll(failed.clone()).unwrap();
        runtime.enroll(healthy.clone()).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.fail_next(AndroidFailure::Inspect);
        let mut runtime = Runtime::new(packages, slots, android);

        let snapshots = runtime.list_packages().unwrap();

        assert_eq!(
            snapshots
                .iter()
                .map(|snapshot| snapshot.package.clone())
                .collect::<Vec<_>>(),
            vec![healthy]
        );
    }

    #[test]
    fn enrolling_an_existing_package_reloads_it() {
        let package = package();
        let mut runtime = runtime();
        let first = runtime.enroll(package.clone()).unwrap();

        let second = runtime.enroll(package).unwrap();

        assert_eq!(second, first);
    }

    #[test]
    fn save_intent_failure_does_not_touch_slot_storage() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone()).unwrap();
        let (mut packages, slots, android) = runtime.into_parts();
        packages.fail_next_save();
        let mut runtime = Runtime::new(packages, slots, android);

        let result =
            runtime.create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank);

        assert_eq!(result, Err(RuntimeError::OperationFailed));
        let (_packages, slots, _android) = runtime.into_parts();
        assert!(!slots.contains(&package, &SlotId::numbered(1)));
    }

    #[test]
    fn each_domain_materialization_failure_aborts_creation() {
        for failure in [SlotFailure::MaterializeCe, SlotFailure::MaterializeDe] {
            let package = package();
            let mut runtime = runtime();
            runtime.enroll(package.clone()).unwrap();
            let (packages, mut slots, android) = runtime.into_parts();
            slots.fail_next(failure);
            let mut runtime = Runtime::new(packages, slots, android);

            let result =
                runtime.create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank);

            assert_eq!(result, Err(RuntimeError::OperationFailed));
            let (_packages, slots, _android) = runtime.into_parts();
            assert!(!slots.contains(&package, &SlotId::numbered(1)));
        }
    }

    #[test]
    fn stale_target_discard_failure_aborts_before_materialization() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone()).unwrap();
        let (packages, mut slots, android) = runtime.into_parts();
        slots.fail_next(SlotFailure::Discard);
        let mut runtime = Runtime::new(packages, slots, android);

        let result =
            runtime.create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank);

        assert_eq!(result, Err(RuntimeError::OperationFailed));
        let (packages, slots, _android) = runtime.into_parts();
        assert!(!slots.contains(&package, &SlotId::numbered(1)));
        assert!(packages.load(&package).unwrap().unwrap().is_ready());
    }

    #[test]
    fn interrupted_creation_discards_pair_and_restores_running_app() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone()).unwrap();
        let (mut packages, mut slots, mut android) = runtime.into_parts();
        let mut aggregate = packages.load(&package).unwrap().unwrap();
        let slot = aggregate
            .reserve_slot(DisplayName::new("Work").unwrap())
            .unwrap();
        let target = slot.id().clone();
        aggregate.begin_creation(&slot, true).unwrap();
        packages.save(&aggregate).unwrap();
        slots.materialize(&package, &slot, SeedMode::Blank).unwrap();
        android.force_stop(&package).unwrap();
        let mut runtime = Runtime::new(packages, slots, android);

        let snapshot = runtime.get_package(&package).unwrap();

        assert_eq!(snapshot.slots.len(), 1);
        let (_packages, slots, android) = runtime.into_parts();
        assert!(!slots.contains(&package, &target));
        assert!(android.is_running(&package));
    }

    #[test]
    fn clone_base_checks_the_real_view_as_well_as_the_aggregate() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone()).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.set_view(&package, ObservedView::Slot(SlotId::numbered(8)));
        let mut runtime = Runtime::new(packages, slots, android);

        let result = runtime.create_slot(
            &package,
            DisplayName::new("Base copy").unwrap(),
            SeedMode::CloneBase,
        );

        assert_eq!(result, Err(RuntimeError::StateConflict));
    }

    #[test]
    fn incomplete_target_is_rejected_without_changing_previous() {
        let package = package();
        let (runtime, target) = with_slot();
        let (packages, mut slots, android) = runtime.into_parts();
        slots.remove_de(&package, &target);
        let mut runtime = Runtime::new(packages, slots, android);

        let result = runtime.activate_slot(&package, &target);

        assert_eq!(result, Err(RuntimeError::StateConflict));
        let (_packages, _slots, android) = runtime.into_parts();
        assert_eq!(android.view(&package), Some(&ObservedView::Base));
    }

    #[test]
    fn apply_failure_rolls_back_previous_and_never_leaves_half_state() {
        let package = package();
        let (runtime, target) = with_slot();
        let (packages, slots, mut android) = runtime.into_parts();
        android.fail_next(AndroidFailure::Apply);
        let mut runtime = Runtime::new(packages, slots, android);

        let result = runtime.activate_slot(&package, &target);

        assert_eq!(result, Err(RuntimeError::OperationFailed));
        let (packages, _slots, android) = runtime.into_parts();
        assert_eq!(android.view(&package), Some(&ObservedView::Base));
        assert_eq!(
            packages.load(&package).unwrap().unwrap().active_slot(),
            &SlotId::base()
        );
    }

    #[test]
    fn force_stop_failure_restores_the_previous_running_state() {
        let package = package();
        let (runtime, target) = with_slot();
        let (packages, slots, mut android) = runtime.into_parts();
        android.fail_next(AndroidFailure::ForceStop);
        let mut runtime = Runtime::new(packages, slots, android);

        let result = runtime.activate_slot(&package, &target);

        assert_eq!(result, Err(RuntimeError::OperationFailed));
        let (packages, _slots, android) = runtime.into_parts();
        assert!(android.is_running(&package));
        let aggregate = packages.load(&package).unwrap().unwrap();
        assert!(aggregate.is_ready());
        assert_eq!(aggregate.active_slot(), &SlotId::base());
    }

    #[test]
    fn invalid_persisted_aggregate_is_a_state_conflict() {
        let root = tempfile::tempdir().unwrap();
        let package = package();
        let packages = FilePackageStore::open(root.path()).unwrap();
        let package_root = root.path().join("packages").join(package.as_str());
        std::fs::create_dir_all(&package_root).unwrap();
        std::fs::write(package_root.join("aggregate.json"), b"{not-json").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(package.clone());
        let mut runtime = Runtime::new(packages, MemorySlotStorage::default(), android);

        assert_eq!(
            runtime.get_package(&package),
            Err(RuntimeError::StateConflict)
        );
    }

    #[test]
    fn rollback_failure_keeps_activation_context_for_later_convergence() {
        let package = package();
        let (runtime, target) = with_slot();
        let (packages, slots, mut android) = runtime.into_parts();
        android.fail_next(AndroidFailure::Apply);
        android.fail_next(AndroidFailure::Apply);
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.activate_slot(&package, &target),
            Err(RuntimeError::OperationFailed)
        );
        assert_eq!(
            runtime.get_package(&package),
            Err(RuntimeError::StateConflict)
        );
    }

    #[test]
    fn interrupted_activation_converges_from_target_or_previous() {
        for observed_target in [false, true] {
            let package = package();
            let (runtime, target) = with_slot();
            let (mut packages, slots, mut android) = runtime.into_parts();
            let mut aggregate = packages.load(&package).unwrap().unwrap();
            aggregate.begin_activation(&target).unwrap();
            packages.save(&aggregate).unwrap();
            android.force_stop(&package).unwrap();
            if observed_target {
                android.apply_view(&package, &target).unwrap();
            }
            let mut runtime = Runtime::new(packages, slots, android);

            assert_eq!(
                runtime.get_package(&package),
                Err(RuntimeError::StateConflict)
            );
            let snapshot = runtime.get_package(&package).unwrap();

            let expected = if observed_target {
                target.clone()
            } else {
                SlotId::base()
            };
            assert_eq!(snapshot.active_slot, expected);
            let (_packages, _slots, android) = runtime.into_parts();
            assert!(android.is_running(&package));
        }
    }

    #[test]
    fn ready_slot_recovers_when_runtime_mounts_return_to_base() {
        let package = package();
        let (mut runtime, target) = with_slot();
        runtime.activate_slot(&package, &target).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.force_stop(&package).unwrap();
        android.set_view(&package, ObservedView::Base);
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        let snapshot = runtime.get_package(&package).unwrap();

        assert_eq!(snapshot.active_slot, target);
        let (_packages, _slots, android) = runtime.into_parts();
        assert_eq!(
            android.view(&package),
            Some(&ObservedView::Slot(target.clone()))
        );
        assert!(!android.is_running(&package));
        assert_eq!(
            &android.calls()[starting_calls..],
            &[
                AndroidCall::Inspect(package.clone()),
                AndroidCall::Observe(package.clone()),
                AndroidCall::ForceStop(package.clone()),
                AndroidCall::Apply(package.clone(), target.clone()),
                AndroidCall::Observe(package),
            ]
        );
    }

    #[test]
    fn changed_package_identity_blocks_old_slots() {
        let package = package();
        let (runtime, target) = with_slot();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&package);
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.activate_slot(&package, &target),
            Err(RuntimeError::StateConflict)
        );
    }

    #[test]
    fn launch_failure_keeps_committed_target_and_same_target_retries_launch() {
        let package = package();
        let (runtime, target) = with_slot();
        let (packages, slots, mut android) = runtime.into_parts();
        android.fail_next(AndroidFailure::Launch);
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.activate_slot(&package, &target),
            Err(RuntimeError::OperationFailed)
        );
        let snapshot = runtime.activate_slot(&package, &target).unwrap();

        assert_eq!(snapshot.active_slot, target);
    }

    #[test]
    fn activation_order_is_intent_stop_apply_verify_commit_launch() {
        let package = package();
        let (runtime, target) = with_slot();
        let (packages, slots, android) = runtime.into_parts();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        runtime.activate_slot(&package, &target).unwrap();

        let (_packages, _slots, android) = runtime.into_parts();
        assert_eq!(
            &android.calls()[starting_calls..],
            &[
                AndroidCall::Inspect(package.clone()),
                AndroidCall::Observe(package.clone()),
                AndroidCall::Observe(package.clone()),
                AndroidCall::ForceStop(package.clone()),
                AndroidCall::Apply(package.clone(), target.clone()),
                AndroidCall::Observe(package.clone()),
                AndroidCall::Launch(package, target),
            ]
        );
    }

    #[test]
    fn final_save_failure_is_recovered_from_the_real_target_view() {
        let package = package();
        let (runtime, target) = with_slot();
        let (mut packages, slots, android) = runtime.into_parts();
        packages.fail_save_call(packages.save_calls() + 2);
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.activate_slot(&package, &target),
            Err(RuntimeError::OperationFailed)
        );
        assert_eq!(
            runtime.get_package(&package),
            Err(RuntimeError::StateConflict)
        );
        let recovered = runtime.get_package(&package).unwrap();

        assert_eq!(recovered.active_slot, target);
    }
}
