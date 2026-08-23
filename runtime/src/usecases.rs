use thiserror::Error;

use crate::model::{
    BindingState, Capabilities, DisplayName, ModelError, ObservedView, PackageAggregate,
    PackageBinding, PackageInspection, PackageName, PackageSnapshot, RebindIntent, SeedMode,
    SigningIdentity, SlotId,
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
    #[error("installed package identity does not match the protected registration")]
    IdentityMismatch,
    #[error("runtime operation failed")]
    OperationFailed,
}

#[derive(Debug)]
pub struct Runtime<P, S, A> {
    packages: P,
    slots: S,
    android: A,
}

#[derive(Debug, Clone, Copy)]
enum UnenrollContext {
    Request,
    Recovery,
}

impl UnenrollContext {
    fn operation(self) -> &'static str {
        match self {
            Self::Request => "unenroll",
            Self::Recovery => "recover",
        }
    }

    fn map_adapter(self, package: &PackageName, step: &str, error: AdapterError) -> RuntimeError {
        log_adapter(self.operation(), package, step, &error);
        match self {
            Self::Request => adapter_error(error),
            Self::Recovery => RuntimeError::StateConflict,
        }
    }

    fn view_failure(self, package: &PackageName) -> RuntimeError {
        eprintln!(
            "op={} package={package} step=verify_unenroll_base error=view_is_not_base",
            self.operation()
        );
        match self {
            Self::Request => RuntimeError::OperationFailed,
            Self::Recovery => RuntimeError::StateConflict,
        }
    }
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
            match self.package_snapshot(&package) {
                Ok(snapshot) => snapshots.push(snapshot),
                Err(error) => {
                    eprintln!("op=list_packages package={package} step=load_package error={error}");
                }
            }
        }
        Ok(snapshots)
    }

    pub fn get_package(&mut self, package: &PackageName) -> Result<PackageSnapshot, RuntimeError> {
        self.package_snapshot(package)
    }

    pub fn enroll(
        &mut self,
        package: PackageName,
        reset: bool,
        signing: Option<SigningIdentity>,
    ) -> Result<PackageSnapshot, RuntimeError> {
        if reset {
            return Err(RuntimeError::InvalidRequest);
        }
        if let Some(signing) = &signing {
            signing.validate().map_err(model_error)?;
        }
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
        let binding_state = if let Some(signing) = signing {
            let binding = PackageBinding::v1(signing).map_err(model_error)?;
            self.packages
                .save_binding(&package, &binding)
                .map_err(|error| logged_adapter("enroll", &package, "save_binding", error))?;
            BindingState::Ready
        } else {
            BindingState::LegacyUnbound
        };
        aggregate
            .snapshot_with_binding(binding_state)
            .map_err(model_error)
    }

    pub fn rebind_package(
        &mut self,
        package: &PackageName,
        signing: SigningIdentity,
        trust_legacy: bool,
    ) -> Result<PackageSnapshot, RuntimeError> {
        signing.validate().map_err(model_error)?;
        let aggregate = self
            .packages
            .load(package)
            .map_err(adapter_error)?
            .ok_or(RuntimeError::NotFound)?;
        aggregate.validate().map_err(model_error)?;
        if !aggregate.is_ready() {
            return Err(RuntimeError::StateConflict);
        }
        let inspection = self
            .android
            .inspect(package)
            .map_err(|error| logged_adapter("rebind_package", package, "inspect", error))?;
        if aggregate.identity().uid() != inspection.identity.uid() {
            return Err(RuntimeError::IdentityMismatch);
        }

        let persisted_binding = self.packages.load_binding(package).map_err(adapter_error)?;
        let pending = self
            .packages
            .load_rebind_intent(package)
            .map_err(adapter_error)?;
        if let Some(intent) = pending {
            if intent.target_identity != inspection.identity
                || !signing.is_compatible_with(&intent.signing)
            {
                return Err(RuntimeError::IdentityMismatch);
            }
            return self.finish_rebind(aggregate, intent);
        }

        if aggregate.identity() == &inspection.identity {
            if let Some(binding) = persisted_binding {
                if !signing.is_compatible_with(binding.signing()) {
                    return Err(RuntimeError::IdentityMismatch);
                }
                self.validate_rebind_preconditions(&aggregate)?;
                return aggregate
                    .snapshot_with_binding(BindingState::Ready)
                    .map_err(model_error);
            }
            if trust_legacy {
                return Err(RuntimeError::InvalidRequest);
            }
            let complete_pairs = self.validate_rebind_preconditions(&aggregate)?;
            self.packages
                .backup_before_v1_binding(&aggregate, &complete_pairs)
                .map_err(|error| {
                    logged_adapter("rebind_package", package, "backup_legacy", error)
                })?;
            let binding = PackageBinding::v1(signing).map_err(model_error)?;
            self.packages
                .save_binding(package, &binding)
                .map_err(|error| {
                    logged_adapter("rebind_package", package, "save_legacy_binding", error)
                })?;
            return aggregate
                .snapshot_with_binding(BindingState::Ready)
                .map_err(model_error);
        }

        match persisted_binding {
            Some(binding) => {
                if trust_legacy {
                    return Err(RuntimeError::InvalidRequest);
                }
                if !signing.is_compatible_with(binding.signing()) {
                    return Err(RuntimeError::IdentityMismatch);
                }
            }
            None if !trust_legacy => return Err(RuntimeError::StateConflict),
            None => {}
        }

        let complete_pairs = self.validate_rebind_preconditions(&aggregate)?;

        self.packages
            .backup_before_v1_binding(&aggregate, &complete_pairs)
            .map_err(|error| logged_adapter("rebind_package", package, "backup", error))?;
        let intent = RebindIntent {
            package: package.clone(),
            previous_identity: aggregate.identity().clone(),
            target_identity: inspection.identity,
            signing,
            active_slot: aggregate.active_slot().clone(),
        };
        intent.validate().map_err(model_error)?;
        self.packages
            .save_rebind_intent(&intent)
            .map_err(|error| logged_adapter("rebind_package", package, "save_intent", error))?;
        self.finish_rebind(aggregate, intent)
    }

    pub fn unenroll(&mut self, package: &PackageName) -> Result<(), RuntimeError> {
        if self
            .packages
            .load(package)
            .map_err(adapter_error)?
            .is_none()
        {
            return Ok(());
        }
        let mut aggregate = self
            .packages
            .load(package)
            .map_err(adapter_error)?
            .ok_or(RuntimeError::NotFound)?;
        aggregate.validate().map_err(model_error)?;
        if aggregate.is_unenrolling() {
            return self.finish_unenrollment(package, UnenrollContext::Request);
        }
        aggregate.begin_unenrollment().map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("unenroll", package, "save_intent", error))?;
        self.finish_unenrollment(package, UnenrollContext::Request)
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

    pub fn rename_slot(
        &mut self,
        package: &PackageName,
        target: &SlotId,
        display_name: DisplayName,
    ) -> Result<PackageSnapshot, RuntimeError> {
        let (mut aggregate, _inspection) = self.load_validated(package)?;
        aggregate
            .rename_slot(target, display_name)
            .map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("rename_slot", package, "save", error))?;
        aggregate.snapshot().map_err(model_error)
    }

    pub fn delete_slot(
        &mut self,
        package: &PackageName,
        target: &SlotId,
    ) -> Result<PackageSnapshot, RuntimeError> {
        let (mut aggregate, _inspection) = self.load_ready(package)?;
        aggregate.begin_deletion(target).map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("delete_slot", package, "save_intent", error))?;
        self.slots
            .discard(package, target)
            .map_err(|error| logged_adapter("delete_slot", package, "discard", error))?;
        aggregate.finish_deletion(target).map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("delete_slot", package, "save_commit", error))?;
        aggregate.snapshot().map_err(model_error)
    }

    pub fn set_launch_after_reboot(
        &mut self,
        package: &PackageName,
        enabled: bool,
    ) -> Result<PackageSnapshot, RuntimeError> {
        let (mut aggregate, _inspection) = self.load_ready(package)?;
        aggregate
            .set_launch_after_reboot(enabled)
            .map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("set_launch_after_reboot", package, "save", error))?;
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
    pub(crate) fn into_parts(self) -> (P, S, A) {
        (self.packages, self.slots, self.android)
    }

    fn load_ready(
        &mut self,
        package: &PackageName,
    ) -> Result<(PackageAggregate, PackageInspection), RuntimeError> {
        let (aggregate, inspection) = self.load_validated(package)?;
        self.finish_ready_load(aggregate, inspection)
    }

    fn finish_ready_load(
        &mut self,
        mut aggregate: PackageAggregate,
        inspection: PackageInspection,
    ) -> Result<(PackageAggregate, PackageInspection), RuntimeError> {
        let package = aggregate.package().clone();
        if aggregate.is_unenrolling() {
            self.finish_unenrollment(&package, UnenrollContext::Recovery)?;
            return Err(RuntimeError::NotFound);
        }
        let recovering_activation = aggregate.pending_activation().is_some();
        self.recover(&mut aggregate)?;
        if recovering_activation {
            return Err(RuntimeError::StateConflict);
        }
        self.converge_ready_view(&package, &aggregate)?;
        Ok((aggregate, inspection))
    }

    fn package_snapshot(&mut self, package: &PackageName) -> Result<PackageSnapshot, RuntimeError> {
        let aggregate = self
            .packages
            .load(package)
            .map_err(adapter_error)?
            .ok_or(RuntimeError::NotFound)?;
        aggregate.validate().map_err(model_error)?;
        let inspection = self
            .android
            .inspect(package)
            .map_err(|error| logged_adapter("load", package, "inspect", error))?;
        let binding = self.packages.load_binding(package).map_err(adapter_error)?;
        let intent = self
            .packages
            .load_rebind_intent(package)
            .map_err(adapter_error)?;
        if intent.is_some() || aggregate.identity() != &inspection.identity {
            let state =
                if binding.is_some() || aggregate.identity().uid() != inspection.identity.uid() {
                    BindingState::RebindRequired
                } else {
                    BindingState::LegacyConfirmationRequired
                };
            return aggregate.snapshot_with_binding(state).map_err(model_error);
        }
        let state = if binding.is_some() {
            BindingState::Ready
        } else {
            BindingState::LegacyUnbound
        };
        let (aggregate, _inspection) = self.finish_ready_load(aggregate, inspection)?;
        aggregate.snapshot_with_binding(state).map_err(model_error)
    }

    fn validate_rebind_preconditions(
        &mut self,
        aggregate: &PackageAggregate,
    ) -> Result<Vec<SlotId>, RuntimeError> {
        if !aggregate.is_ready() {
            return Err(RuntimeError::StateConflict);
        }
        let package = aggregate.package();
        let complete_pairs = aggregate
            .snapshot()
            .map_err(model_error)?
            .slots
            .into_iter()
            .filter_map(|slot| (!slot.id.is_base()).then_some(slot.id))
            .collect::<Vec<_>>();
        for slot in &complete_pairs {
            self.slots
                .require_complete_pair(package, slot)
                .map_err(|error| {
                    logged_conflict("rebind_package", package, "verify_slot_pair", error)
                })?;
        }
        let observed = self
            .android
            .observe_view(package)
            .map_err(|error| logged_conflict("rebind_package", package, "observe_view", error))?;
        if observed != ObservedView::Base && !observed.matches(aggregate.active_slot()) {
            return Err(RuntimeError::StateConflict);
        }
        Ok(complete_pairs)
    }

    fn finish_rebind(
        &mut self,
        mut aggregate: PackageAggregate,
        intent: RebindIntent,
    ) -> Result<PackageSnapshot, RuntimeError> {
        let package = &intent.package;
        self.android
            .force_stop(package)
            .map_err(|error| logged_adapter("rebind_package", package, "force_stop", error))?;
        let base = SlotId::base();
        self.android
            .apply_view(package, &base)
            .map_err(|error| logged_adapter("rebind_package", package, "apply_base", error))?;
        let base_view = self
            .android
            .observe_view(package)
            .map_err(|error| logged_adapter("rebind_package", package, "verify_base", error))?;
        if base_view != ObservedView::Base {
            return Err(RuntimeError::OperationFailed);
        }
        if aggregate.identity() != &intent.target_identity {
            aggregate
                .rebind_identity(intent.target_identity.clone())
                .map_err(model_error)?;
            self.packages.save(&aggregate).map_err(|error| {
                logged_adapter("rebind_package", package, "save_identity", error)
            })?;
        }
        let binding = PackageBinding::v1(intent.signing).map_err(model_error)?;
        self.packages
            .save_binding(package, &binding)
            .map_err(|error| logged_adapter("rebind_package", package, "save_binding", error))?;
        if !intent.active_slot.is_base() {
            self.slots
                .require_complete_pair(package, &intent.active_slot)
                .map_err(|error| {
                    logged_conflict("rebind_package", package, "verify_active_pair", error)
                })?;
            self.android
                .apply_view(package, &intent.active_slot)
                .map_err(|error| {
                    logged_adapter("rebind_package", package, "restore_active", error)
                })?;
            let restored = self.android.observe_view(package).map_err(|error| {
                logged_adapter("rebind_package", package, "verify_active", error)
            })?;
            if !restored.matches(&intent.active_slot) {
                return Err(RuntimeError::OperationFailed);
            }
        }
        self.packages
            .clear_rebind_intent(package)
            .map_err(|error| logged_adapter("rebind_package", package, "clear_intent", error))?;
        aggregate
            .snapshot_with_binding(BindingState::Ready)
            .map_err(model_error)
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
            if aggregate.launch_after_reboot() {
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
        if let Some(target) = aggregate.pending_deletion().cloned() {
            let package = aggregate.package().clone();
            let observed = self
                .android
                .observe_view(&package)
                .map_err(|error| logged_conflict("recover", &package, "observe_deleting", error))?;
            if observed == ObservedView::Inconsistent || observed.matches(&target) {
                return Err(RuntimeError::StateConflict);
            }
            self.slots
                .discard(&package, &target)
                .map_err(|error| logged_conflict("recover", &package, "discard_deleting", error))?;
            aggregate.finish_deletion(&target).map_err(model_error)?;
            self.packages
                .save(aggregate)
                .map_err(|error| logged_conflict("recover", &package, "save_deleting", error))?;
        }

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

    fn finish_unenrollment(
        &mut self,
        package: &PackageName,
        context: UnenrollContext,
    ) -> Result<(), RuntimeError> {
        self.android
            .force_stop(package)
            .map_err(|error| context.map_adapter(package, "force_stop_unenroll", error))?;
        let base = SlotId::base();
        self.android
            .apply_view(package, &base)
            .map_err(|error| context.map_adapter(package, "apply_unenroll_base", error))?;
        let observed = self
            .android
            .observe_view(package)
            .map_err(|error| context.map_adapter(package, "verify_unenroll_base", error))?;
        if observed != ObservedView::Base {
            return Err(context.view_failure(package));
        }
        self.slots
            .discard_package(package)
            .map_err(|error| context.map_adapter(package, "discard_unenroll_slots", error))?;
        self.packages
            .remove(package)
            .map_err(|error| context.map_adapter(package, "remove_unenroll_state", error))
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
        ModelError::InvalidPackage
        | ModelError::InvalidSlot
        | ModelError::InvalidDisplayName
        | ModelError::InvalidSigningIdentity => RuntimeError::InvalidRequest,
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

    fn signing() -> SigningIdentity {
        SigningIdentity::new(crate::model::SigningKind::Lineage, vec!["a".repeat(64)]).unwrap()
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
        runtime.enroll(package.clone(), false, None).unwrap();
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
        runtime.enroll(failed.clone(), false, None).unwrap();
        runtime.enroll(healthy.clone(), false, None).unwrap();
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
        let first = runtime.enroll(package.clone(), false, None).unwrap();

        let second = runtime.enroll(package, false, None).unwrap();

        assert_eq!(second, first);
    }

    #[test]
    fn rename_is_metadata_only_and_allows_the_active_non_base_slot() {
        let package = package();
        let (mut runtime, target) = with_slot();
        runtime.activate_slot(&package, &target).unwrap();
        let (packages, slots, android) = runtime.into_parts();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        let renamed = runtime
            .rename_slot(
                &package,
                &target,
                DisplayName::new("Current account").unwrap(),
            )
            .unwrap();

        assert_eq!(renamed.slots[1].name, "Current account");
        let (_packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
        assert_eq!(
            &android.calls()[starting_calls..],
            &[AndroidCall::Inspect(package)]
        );
    }

    #[test]
    fn rename_save_failure_is_atomic_and_non_ready_state_is_not_recovered() {
        let package = package();
        let (runtime, target) = with_slot();
        let (mut packages, slots, android) = runtime.into_parts();
        packages.fail_next_save();
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.rename_slot(&package, &target, DisplayName::new("Changed").unwrap()),
            Err(RuntimeError::OperationFailed)
        );

        let (mut packages, slots, android) = runtime.into_parts();
        assert_eq!(
            packages
                .load(&package)
                .unwrap()
                .unwrap()
                .snapshot()
                .unwrap()
                .slots[1]
                .name,
            "Work"
        );
        let mut deleting = packages.load(&package).unwrap().unwrap();
        deleting.begin_deletion(&target).unwrap();
        packages.save(&deleting).unwrap();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.rename_slot(&package, &target, DisplayName::new("Blocked").unwrap()),
            Err(RuntimeError::StateConflict)
        );
        let (packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
        assert!(
            packages
                .load(&package)
                .unwrap()
                .unwrap()
                .pending_deletion()
                .is_some()
        );
        assert_eq!(
            &android.calls()[starting_calls..],
            &[AndroidCall::Inspect(package)]
        );
    }

    #[test]
    fn delete_rejects_base_missing_and_active_slots() {
        let package = package();
        let (mut runtime, target) = with_slot();

        assert_eq!(
            runtime.delete_slot(&package, &SlotId::base()),
            Err(RuntimeError::StateConflict)
        );
        assert_eq!(
            runtime.delete_slot(&package, &SlotId::numbered(9)),
            Err(RuntimeError::NotFound)
        );
        runtime.activate_slot(&package, &target).unwrap();
        assert_eq!(
            runtime.delete_slot(&package, &target),
            Err(RuntimeError::StateConflict)
        );
    }

    #[test]
    fn delete_discards_the_inactive_pair_without_reusing_its_number() {
        let package = package();
        let (mut runtime, target) = with_slot();

        let deleted = runtime.delete_slot(&package, &target).unwrap();

        assert_eq!(deleted.slots.len(), 1);
        let (packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), None);
        let mut runtime = Runtime::new(packages, slots, android);
        let recreated = runtime
            .create_slot(&package, DisplayName::new("Next").unwrap(), SeedMode::Blank)
            .unwrap();
        assert_eq!(recreated.slots[1].id, SlotId::numbered(2));
    }

    #[test]
    fn delete_persists_intent_before_discarding_storage() {
        let package = package();
        let (runtime, target) = with_slot();
        let (mut packages, slots, android) = runtime.into_parts();
        packages.fail_next_save();
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.delete_slot(&package, &target),
            Err(RuntimeError::OperationFailed)
        );

        let (packages, slots, _android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
        let aggregate = packages.load(&package).unwrap().unwrap();
        assert!(aggregate.is_ready());
        assert!(aggregate.has_slot(&target));
    }

    #[test]
    fn partial_domain_delete_is_operation_failed_then_recovers_idempotently() {
        for (failure, remaining) in [
            (SlotFailure::DiscardCe, (true, false)),
            (SlotFailure::DiscardDe, (false, true)),
        ] {
            let package = package();
            let (runtime, target) = with_slot();
            let (packages, mut slots, android) = runtime.into_parts();
            slots.fail_next(failure);
            let mut runtime = Runtime::new(packages, slots, android);

            assert_eq!(
                runtime.delete_slot(&package, &target),
                Err(RuntimeError::OperationFailed)
            );
            let (packages, slots, android) = runtime.into_parts();
            assert_eq!(slots.domain_state(&package, &target), Some(remaining));
            assert_eq!(
                packages.load(&package).unwrap().unwrap().pending_deletion(),
                Some(&target)
            );

            let mut runtime = Runtime::new(packages, slots, android);
            let recovered = runtime.get_package(&package).unwrap();
            assert_eq!(recovered.slots.len(), 1);
            let (packages, slots, android) = runtime.into_parts();
            assert_eq!(slots.domain_state(&package, &target), None);
            let mut runtime = Runtime::new(packages, slots, android);
            let recreated = runtime
                .create_slot(&package, DisplayName::new("Next").unwrap(), SeedMode::Blank)
                .unwrap();
            assert_eq!(recreated.slots[1].id, SlotId::numbered(2));
        }
    }

    #[test]
    fn delete_recovery_failure_is_state_conflict_and_can_retry() {
        let package = package();
        let (runtime, target) = with_slot();
        let (packages, mut slots, android) = runtime.into_parts();
        slots.fail_next(SlotFailure::DiscardCe);
        slots.fail_next(SlotFailure::Discard);
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.delete_slot(&package, &target),
            Err(RuntimeError::OperationFailed)
        );
        assert_eq!(
            runtime.get_package(&package),
            Err(RuntimeError::StateConflict)
        );

        let recovered = runtime.get_package(&package).unwrap();
        assert_eq!(recovered.slots.len(), 1);
        let (_packages, slots, _android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), None);
    }

    #[test]
    fn delete_commit_save_failure_recovers_after_idempotent_discard() {
        let package = package();
        let (runtime, target) = with_slot();
        let (mut packages, slots, android) = runtime.into_parts();
        let next_save = packages.save_calls() + 1;
        packages.fail_save_call(next_save + 1);
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.delete_slot(&package, &target),
            Err(RuntimeError::OperationFailed)
        );
        let (packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), None);
        assert_eq!(
            packages.load(&package).unwrap().unwrap().pending_deletion(),
            Some(&target)
        );

        let mut runtime = Runtime::new(packages, slots, android);
        let recovered = runtime.get_package(&package).unwrap();
        assert_eq!(recovered.slots.len(), 1);
    }

    #[test]
    fn delete_recovery_never_discards_the_real_target_or_an_inconsistent_view() {
        for observed in [
            ObservedView::Slot(SlotId::numbered(1)),
            ObservedView::Inconsistent,
        ] {
            let package = package();
            let (runtime, target) = with_slot();
            let (mut packages, slots, mut android) = runtime.into_parts();
            let mut aggregate = packages.load(&package).unwrap().unwrap();
            aggregate.begin_deletion(&target).unwrap();
            packages.save(&aggregate).unwrap();
            android.set_view(&package, observed);
            let mut runtime = Runtime::new(packages, slots, android);

            assert_eq!(
                runtime.get_package(&package),
                Err(RuntimeError::StateConflict)
            );

            let (packages, slots, _android) = runtime.into_parts();
            assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
            assert_eq!(
                packages.load(&package).unwrap().unwrap().pending_deletion(),
                Some(&target)
            );
        }
    }

    #[test]
    fn legacy_identity_change_stays_listed_until_confirmed_without_losing_slots() {
        let package = package();
        let (mut runtime, target) = with_slot();
        runtime.activate_slot(&package, &target).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&package);
        let mut runtime = Runtime::new(packages, slots, android);

        let listed = runtime.list_packages().unwrap();

        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].binding_state,
            BindingState::LegacyConfirmationRequired
        );
        assert_eq!(listed[0].active_slot, target);
        assert_eq!(listed[0].slots.len(), 2);
        let rebound = runtime.rebind_package(&package, signing(), true).unwrap();
        assert_eq!(rebound.binding_state, BindingState::Ready);
        assert_eq!(rebound.active_slot, target);
        assert_eq!(rebound.slots.len(), 2);
        let (_packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
        assert_eq!(android.view(&package), Some(&ObservedView::Slot(target)));
        assert!(!android.is_running(&package));
    }

    #[test]
    fn same_signing_identity_rebinds_future_apk_replacements_without_confirmation() {
        let package = package();
        let mut runtime = runtime();
        runtime
            .enroll(package.clone(), false, Some(signing()))
            .unwrap();
        let created = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let target = created.slots[1].id.clone();
        runtime.activate_slot(&package, &target).unwrap();
        runtime.set_launch_after_reboot(&package, true).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&package);
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.list_packages().unwrap()[0].binding_state,
            BindingState::RebindRequired
        );
        let rebound = runtime.rebind_package(&package, signing(), false).unwrap();

        assert_eq!(rebound.active_slot, target);
        assert!(rebound.launch_after_reboot);
        assert_eq!(rebound.slots[1].name, "Work");
        let (packages, slots, mut android) = runtime.into_parts();
        assert!(slots.contains(&package, &target));
        assert!(!android.is_running(&package));
        android.change_identity_to(&package, 2);
        let mut runtime = Runtime::new(packages, slots, android);
        assert_eq!(
            runtime.list_packages().unwrap()[0].binding_state,
            BindingState::RebindRequired
        );

        let downgraded = runtime.rebind_package(&package, signing(), false).unwrap();

        assert_eq!(downgraded.active_slot, target);
        assert!(downgraded.launch_after_reboot);
        assert_eq!(downgraded.slots[1].name, "Work");
        let (_packages, slots, android) = runtime.into_parts();
        assert!(slots.contains(&package, &downgraded.active_slot));
        assert!(!android.is_running(&package));
    }

    #[test]
    fn package_listing_marks_only_the_app_whose_apk_identity_changed() {
        let changed = PackageName::new("com.example.changed").unwrap();
        let untouched = PackageName::new("com.example.untouched").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(changed.clone());
        android.install(untouched.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        );
        runtime
            .enroll(changed.clone(), false, Some(signing()))
            .unwrap();
        runtime
            .enroll(untouched.clone(), false, Some(signing()))
            .unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&changed);
        let mut runtime = Runtime::new(packages, slots, android);

        let snapshots = runtime.list_packages().unwrap();

        assert_eq!(snapshots.len(), 2);
        assert_eq!(
            snapshots
                .iter()
                .find(|snapshot| snapshot.package == changed)
                .unwrap()
                .binding_state,
            BindingState::RebindRequired
        );
        assert_eq!(
            snapshots
                .iter()
                .find(|snapshot| snapshot.package == untouched)
                .unwrap()
                .binding_state,
            BindingState::Ready
        );
    }

    #[test]
    fn signer_mismatch_protects_old_slots_without_android_side_effects() {
        let package = package();
        let mut runtime = runtime();
        runtime
            .enroll(package.clone(), false, Some(signing()))
            .unwrap();
        let created = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let target = created.slots[1].id.clone();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&package);
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);
        let different =
            SigningIdentity::new(crate::model::SigningKind::Lineage, vec!["b".repeat(64)]).unwrap();

        assert_eq!(
            runtime.rebind_package(&package, different, false),
            Err(RuntimeError::IdentityMismatch)
        );
        let (packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
        assert!(packages.load_rebind_intent(&package).unwrap().is_none());
        assert_eq!(
            &android.calls()[starting_calls..],
            &[AndroidCall::Inspect(package)]
        );
    }

    #[test]
    fn uid_change_is_never_accepted_even_with_the_same_signing_identity() {
        let package = package();
        let mut runtime = runtime();
        runtime
            .enroll(package.clone(), false, Some(signing()))
            .unwrap();
        let created = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let target = created.slots[1].id.clone();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_uid(&package);
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.list_packages().unwrap()[0].binding_state,
            BindingState::RebindRequired
        );

        assert_eq!(
            runtime.rebind_package(&package, signing(), false),
            Err(RuntimeError::IdentityMismatch)
        );

        let (packages, slots, android) = runtime.into_parts();
        assert!(slots.contains(&package, &target));
        assert!(packages.load_rebind_intent(&package).unwrap().is_none());
        assert_eq!(
            &android.calls()[starting_calls..],
            &[
                AndroidCall::Inspect(package.clone()),
                AndroidCall::Inspect(package),
            ]
        );
    }

    #[test]
    fn incomplete_slot_pair_blocks_rebind_before_the_journal_or_mount_changes() {
        let package = package();
        let mut runtime = runtime();
        runtime
            .enroll(package.clone(), false, Some(signing()))
            .unwrap();
        let created = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let target = created.slots[1].id.clone();
        let (packages, mut slots, mut android) = runtime.into_parts();
        slots.remove_de(&package, &target);
        android.change_identity(&package);
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.rebind_package(&package, signing(), false),
            Err(RuntimeError::StateConflict)
        );

        let (packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), Some((true, false)));
        assert!(packages.load_rebind_intent(&package).unwrap().is_none());
        assert_eq!(
            &android.calls()[starting_calls..],
            &[AndroidCall::Inspect(package)]
        );
    }

    #[test]
    fn inconsistent_view_blocks_rebind_before_the_journal_or_mount_changes() {
        let package = package();
        let mut runtime = runtime();
        runtime
            .enroll(package.clone(), false, Some(signing()))
            .unwrap();
        let created = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let target = created.slots[1].id.clone();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&package);
        android.set_view(&package, ObservedView::Inconsistent);
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.rebind_package(&package, signing(), false),
            Err(RuntimeError::StateConflict)
        );

        let (packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
        assert!(packages.load_rebind_intent(&package).unwrap().is_none());
        assert_eq!(
            &android.calls()[starting_calls..],
            &[
                AndroidCall::Inspect(package.clone()),
                AndroidCall::Observe(package),
            ]
        );
    }

    #[test]
    fn legacy_identity_match_silently_adds_the_sidecar_and_inventory_backup() {
        let package = package();
        let (mut runtime, target) = with_slot();
        let listed = runtime.list_packages().unwrap();
        assert_eq!(listed[0].binding_state, BindingState::LegacyUnbound);
        let (packages, slots, android) = runtime.into_parts();
        let original = packages.load(&package).unwrap().unwrap();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        let rebound = runtime.rebind_package(&package, signing(), false).unwrap();

        assert_eq!(rebound.binding_state, BindingState::Ready);
        let (packages, slots, android) = runtime.into_parts();
        assert_eq!(packages.load(&package).unwrap().unwrap(), original);
        assert!(packages.load_binding(&package).unwrap().is_some());
        let (backup, pairs) = packages.backup(&package).unwrap();
        assert_eq!(backup, &original);
        assert_eq!(pairs, &vec![target.clone()]);
        assert!(slots.contains(&package, &target));
        assert!(android.is_running(&package));
        assert!(
            android.calls()[starting_calls..]
                .iter()
                .all(|call| !matches!(call, AndroidCall::Launch(..)))
        );
    }

    #[test]
    fn interrupted_rebind_keeps_the_journal_and_resumes_without_launching() {
        let package = package();
        let mut runtime = runtime();
        runtime
            .enroll(package.clone(), false, Some(signing()))
            .unwrap();
        let created = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let target = created.slots[1].id.clone();
        runtime.activate_slot(&package, &target).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.change_identity(&package);
        android.fail_next(AndroidFailure::ForceStop);
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.rebind_package(&package, signing(), false),
            Err(RuntimeError::OperationFailed)
        );
        let (packages, slots, android) = runtime.into_parts();
        assert!(packages.load_rebind_intent(&package).unwrap().is_some());
        assert!(slots.contains(&package, &target));
        let mut runtime = Runtime::new(packages, slots, android);

        let resumed = runtime.rebind_package(&package, signing(), false).unwrap();

        assert_eq!(resumed.active_slot, target);
        let (packages, slots, android) = runtime.into_parts();
        assert!(packages.load_rebind_intent(&package).unwrap().is_none());
        assert!(slots.contains(&package, &resumed.active_slot));
        assert!(!android.is_running(&package));
        assert!(
            android.calls()[starting_calls..]
                .iter()
                .all(|call| !matches!(call, AndroidCall::Launch(..)))
        );
    }

    #[test]
    fn old_destructive_reset_command_is_rejected() {
        let package = package();
        let (mut runtime, target) = with_slot();

        assert_eq!(
            runtime.enroll(package.clone(), true, None),
            Err(RuntimeError::InvalidRequest)
        );
        let (_packages, slots, _android) = runtime.into_parts();
        assert!(slots.contains(&package, &target));
    }

    #[test]
    fn save_intent_failure_does_not_touch_slot_storage() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
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
            runtime.enroll(package.clone(), false, None).unwrap();
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
        runtime.enroll(package.clone(), false, None).unwrap();
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
        runtime.enroll(package.clone(), false, None).unwrap();
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
        runtime.enroll(package.clone(), false, None).unwrap();
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
    fn ready_slot_launches_after_restore_only_when_enabled() {
        let package = package();
        let (mut runtime, target) = with_slot();
        runtime.activate_slot(&package, &target).unwrap();
        let updated = runtime.set_launch_after_reboot(&package, true).unwrap();
        assert!(updated.launch_after_reboot);
        let (packages, slots, mut android) = runtime.into_parts();
        android.set_view(&package, ObservedView::Base);
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        let snapshot = runtime.get_package(&package).unwrap();

        assert_eq!(snapshot.active_slot, target);
        assert!(snapshot.launch_after_reboot);
        let (_packages, _slots, android) = runtime.into_parts();
        assert!(android.is_running(&package));
        assert_eq!(
            &android.calls()[starting_calls..],
            &[
                AndroidCall::Inspect(package.clone()),
                AndroidCall::Observe(package.clone()),
                AndroidCall::ForceStop(package.clone()),
                AndroidCall::Apply(package.clone(), target.clone()),
                AndroidCall::Observe(package.clone()),
                AndroidCall::Launch(package, target),
            ]
        );
    }

    #[test]
    fn base_never_auto_launches_even_when_enabled() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        runtime.set_launch_after_reboot(&package, true).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.force_stop(&package).unwrap();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        let snapshot = runtime.get_package(&package).unwrap();

        assert_eq!(snapshot.active_slot, SlotId::base());
        assert!(snapshot.launch_after_reboot);
        let (_packages, _slots, android) = runtime.into_parts();
        assert!(!android.is_running(&package));
        assert_eq!(
            &android.calls()[starting_calls..],
            &[
                AndroidCall::Inspect(package.clone()),
                AndroidCall::Observe(package),
            ]
        );
    }

    #[test]
    fn package_listing_launches_only_opted_in_restored_apps() {
        let opted_in = PackageName::new("com.example.opted.in").unwrap();
        let opted_out = PackageName::new("com.example.opted.out").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(opted_in.clone());
        android.install(opted_out.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        );
        let mut targets = Vec::new();
        for package in [&opted_in, &opted_out] {
            runtime.enroll(package.clone(), false, None).unwrap();
            let target = runtime
                .create_slot(package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
                .unwrap()
                .slots[1]
                .id
                .clone();
            runtime.activate_slot(package, &target).unwrap();
            targets.push((package.clone(), target));
        }
        runtime.set_launch_after_reboot(&opted_in, true).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        for (package, _) in &targets {
            android.set_view(package, ObservedView::Base);
        }
        let mut runtime = Runtime::new(packages, slots, android);

        let snapshots = runtime.list_packages().unwrap();

        assert_eq!(snapshots.len(), 2);
        let (_packages, _slots, android) = runtime.into_parts();
        assert!(android.is_running(&opted_in));
        assert!(!android.is_running(&opted_out));
        for (package, target) in targets {
            assert_eq!(android.view(&package), Some(&ObservedView::Slot(target)));
        }
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

    #[test]
    fn unenroll_from_active_slot_returns_to_base_and_removes_every_slot() {
        let package = package();
        let (mut runtime, target) = with_slot();
        runtime.activate_slot(&package, &target).unwrap();
        let (packages, slots, android) = runtime.into_parts();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        runtime.unenroll(&package).unwrap();

        let (packages, slots, android) = runtime.into_parts();
        assert!(packages.load(&package).unwrap().is_none());
        assert_eq!(slots.domain_state(&package, &target), None);
        assert_eq!(android.view(&package), Some(&ObservedView::Base));
        assert!(!android.is_running(&package));
        assert_eq!(
            &android.calls()[starting_calls..],
            &[
                AndroidCall::ForceStop(package.clone()),
                AndroidCall::Apply(package.clone(), SlotId::base()),
                AndroidCall::Observe(package),
            ]
        );
    }

    #[test]
    fn unenroll_from_base_and_repeated_unenroll_are_idempotent() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();

        runtime.unenroll(&package).unwrap();
        runtime.unenroll(&package).unwrap();

        let (packages, _slots, android) = runtime.into_parts();
        assert!(packages.load(&package).unwrap().is_none());
        assert_eq!(android.view(&package), Some(&ObservedView::Base));
        assert!(!android.is_running(&package));
    }

    #[test]
    fn unenroll_save_intent_failure_has_no_destructive_side_effect() {
        let package = package();
        let (runtime, target) = with_slot();
        let (mut packages, slots, android) = runtime.into_parts();
        packages.fail_next_save();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.unenroll(&package),
            Err(RuntimeError::OperationFailed)
        );

        let (packages, slots, android) = runtime.into_parts();
        assert!(packages.load(&package).unwrap().unwrap().is_ready());
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
        assert_eq!(&android.calls()[starting_calls..], &[]);
    }

    #[test]
    fn unenroll_android_failures_keep_intent_and_recover_forward() {
        for failure in [AndroidFailure::ForceStop, AndroidFailure::Apply] {
            let package = package();
            let (mut runtime, target) = with_slot();
            runtime.activate_slot(&package, &target).unwrap();
            let (packages, slots, mut android) = runtime.into_parts();
            android.fail_next(failure);
            let mut runtime = Runtime::new(packages, slots, android);

            assert_eq!(
                runtime.unenroll(&package),
                Err(RuntimeError::OperationFailed)
            );
            let (packages, slots, android) = runtime.into_parts();
            assert!(packages.load(&package).unwrap().unwrap().is_unenrolling());
            assert_eq!(slots.domain_state(&package, &target), Some((true, true)));

            let mut runtime = Runtime::new(packages, slots, android);
            assert_eq!(runtime.get_package(&package), Err(RuntimeError::NotFound));
            let (packages, slots, android) = runtime.into_parts();
            assert!(packages.load(&package).unwrap().is_none());
            assert_eq!(slots.domain_state(&package, &target), None);
            assert_eq!(android.view(&package), Some(&ObservedView::Base));
        }
    }

    #[test]
    fn unenroll_verification_failure_is_retryable_state_conflict() {
        let package = package();
        let (mut runtime, target) = with_slot();
        runtime.activate_slot(&package, &target).unwrap();
        let (mut packages, slots, mut android) = runtime.into_parts();
        let mut aggregate = packages.load(&package).unwrap().unwrap();
        aggregate.begin_unenrollment().unwrap();
        packages.save(&aggregate).unwrap();
        android.fail_next(AndroidFailure::Observe);
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.get_package(&package),
            Err(RuntimeError::StateConflict)
        );
        let (packages, slots, android) = runtime.into_parts();
        assert!(packages.load(&package).unwrap().unwrap().is_unenrolling());
        assert_eq!(slots.domain_state(&package, &target), Some((true, true)));

        let mut runtime = Runtime::new(packages, slots, android);
        assert_eq!(runtime.get_package(&package), Err(RuntimeError::NotFound));
    }

    #[test]
    fn partial_unenroll_storage_cleanup_recovers_both_domains() {
        for (failure, remaining) in [
            (SlotFailure::DiscardPackageCe, (true, false)),
            (SlotFailure::DiscardPackageDe, (false, true)),
        ] {
            let package = package();
            let (runtime, target) = with_slot();
            let (packages, mut slots, android) = runtime.into_parts();
            slots.fail_next(failure);
            let mut runtime = Runtime::new(packages, slots, android);

            assert_eq!(
                runtime.unenroll(&package),
                Err(RuntimeError::OperationFailed)
            );
            let (packages, slots, android) = runtime.into_parts();
            assert_eq!(slots.domain_state(&package, &target), Some(remaining));
            assert!(packages.load(&package).unwrap().unwrap().is_unenrolling());

            let mut runtime = Runtime::new(packages, slots, android);
            assert_eq!(runtime.get_package(&package), Err(RuntimeError::NotFound));
            let (packages, slots, _android) = runtime.into_parts();
            assert!(packages.load(&package).unwrap().is_none());
            assert_eq!(slots.domain_state(&package, &target), None);
        }
    }

    #[test]
    fn unenroll_store_remove_failure_is_completed_by_package_listing() {
        let package = package();
        let (runtime, target) = with_slot();
        let (mut packages, slots, android) = runtime.into_parts();
        packages.fail_next_remove();
        let mut runtime = Runtime::new(packages, slots, android);

        assert_eq!(
            runtime.unenroll(&package),
            Err(RuntimeError::OperationFailed)
        );
        let (packages, slots, android) = runtime.into_parts();
        assert_eq!(slots.domain_state(&package, &target), None);
        assert!(packages.load(&package).unwrap().unwrap().is_unenrolling());

        let mut runtime = Runtime::new(packages, slots, android);
        assert!(runtime.list_packages().unwrap().is_empty());
        let (packages, _slots, _android) = runtime.into_parts();
        assert!(packages.load(&package).unwrap().is_none());
    }
}
