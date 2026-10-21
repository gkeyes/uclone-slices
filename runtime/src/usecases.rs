use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read as _;

use thiserror::Error;

use crate::model::{
    AccountIoIntent, AccountIoKind, AccountIoLease, AccountIoPhase, AccountIoScope,
    AccountIoSource, AccountIoStatus, AccountIoToken, ArchiveAccountId, ArchiveAccountKind,
    ArchiveScope, ArchivedState, BASE_ACCOUNT_NAME, BindingState, Capabilities, DisplayName,
    ModelError, ObservedView, PackageAggregate, PackageBinding, PackageEnabledState,
    PackageInspection, PackageName, PackageSnapshot, RebindIntent, ResolvedRestoreMapping,
    RestoreBatchResult, RestoreItemResult, RestoreItemState, RestoreMapping, RestoreTarget,
    SeedMode, SigningIdentity, SlotId, TransferId, TrashHold, TrashIntent,
};
#[cfg(test)]
use crate::model::{ArchiveDomain, RestorePath, RestorePolicy};
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
    #[error("package account I/O is busy")]
    IoBusy,
    #[error("insufficient storage")]
    InsufficientStorage,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Relaunch {
    NotFailed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadyLoadContext {
    Interactive,
    Boot,
}

impl ReadyLoadContext {
    const fn operation(self) -> &'static str {
        match self {
            Self::Interactive => "load",
            Self::Boot => "reconcile_boot",
        }
    }
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
        let inspections = self.android.inspect_many(&names);
        let mut snapshots = Vec::with_capacity(names.len());
        for (package, inspection) in inspections {
            let snapshot = inspection.map_err(adapter_error).and_then(|inspection| {
                self.package_snapshot_with_inspection(
                    &package,
                    inspection,
                    ReadyLoadContext::Interactive,
                )
            });
            match snapshot {
                Ok(snapshot) => snapshots.push(snapshot),
                // The 0.2.x wire has no degraded list entry, so a package that fails to load is
                // omitted here and logged; `get_package` still reports its own error code.
                // Surfacing it in the list needs a protocol field plus a Manager consumer.
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

    /// Recovers interrupted account I/O, then converges every package. A package that fails
    /// to converge is logged and skipped; it keeps failing closed on its own requests. A failed
    /// account-I/O recovery is reported as an error after the remaining packages are processed,
    /// so the caller does not mark this boot as reconciled and retries the recovery.
    pub fn reconcile_boot(&mut self) -> Result<(), RuntimeError> {
        let recovered = self.recover_account_io_intents();
        let names = self.packages.list().map_err(adapter_error)?;
        for package in names {
            if let Err(error) = self.package_snapshot_with_context(&package, ReadyLoadContext::Boot)
            {
                eprintln!(
                    "op=reconcile_boot package={package} step=reconcile_package error={error}"
                );
            }
        }
        self.purge_trash("reconcile_boot");
        recovered
    }

    pub fn begin_backup_io(
        &mut self,
        package: &PackageName,
        scope: AccountIoScope,
    ) -> Result<AccountIoLease, RuntimeError> {
        self.ensure_no_account_io(package)?;
        let (aggregate, inspection) = self.load_ready(package)?;
        let snapshot = aggregate.snapshot().map_err(model_error)?;
        let requested_slots = match &scope {
            AccountIoScope::Account { slot } => {
                if !aggregate.has_slot(slot) {
                    return Err(RuntimeError::NotFound);
                }
                vec![slot.clone()]
            }
            AccountIoScope::AllAccounts => {
                snapshot.slots.iter().map(|slot| slot.id.clone()).collect()
            }
        };
        for slot in &requested_slots {
            if !slot.is_base() {
                self.slots
                    .require_complete_pair(package, slot)
                    .map_err(|error| {
                        logged_conflict("begin_backup_io", package, "verify_slot_pair", error)
                    })?;
            }
        }
        let maintenance_active = matches!(scope, AccountIoScope::AllAccounts)
            || requested_slots
                .iter()
                .any(|slot| slot.is_base() || slot == aggregate.active_slot());
        let previous_enabled_state = if maintenance_active {
            Some(self.android.read_enabled_state(package).map_err(|error| {
                logged_adapter("begin_backup_io", package, "read_enabled_state", error)
            })?)
        } else {
            None
        };
        let token = generate_account_io_token()?;
        let mut intent = AccountIoIntent {
            schema_version: 2,
            io_token: token.clone(),
            package: package.clone(),
            kind: AccountIoKind::Backup,
            scope: Some(scope),
            transfer_id: None,
            mappings: Vec::new(),
            archived_state: None,
            previous_active_slot: aggregate.active_slot().clone(),
            previous_launch_after_reboot: aggregate.launch_after_reboot(),
            previous_was_running: inspection.was_running,
            previous_enabled_state,
            maintenance_active,
            phase: AccountIoPhase::Preparing,
            completed: Vec::new(),
        };
        self.packages
            .save_account_io_intent(&intent)
            .map_err(|error| logged_adapter("begin_backup_io", package, "save_intent", error))?;

        if maintenance_active {
            if let Err(error) = self
                .android
                .block_launch(package)
                .map_err(|error| logged_adapter("begin_backup_io", package, "block_launch", error))
            {
                return Err(self.abandon_backup_lease(&intent, false, error));
            }
            if let Err(error) = self.apply_view_verified(package, &SlotId::base()) {
                return Err(self.abandon_backup_lease(&intent, true, error));
            }
        }

        let sources = (|| {
            let mut sources = Vec::with_capacity(requested_slots.len());
            for slot in requested_slots {
                let (ce_path, de_path) =
                    self.slots.account_paths(package, &slot).map_err(|error| {
                        logged_adapter("begin_backup_io", package, "resolve_source", error)
                    })?;
                let name = if slot.is_base() {
                    BASE_ACCOUNT_NAME.to_owned()
                } else {
                    snapshot
                        .slots
                        .iter()
                        .find(|candidate| candidate.id == slot)
                        .map(|candidate| candidate.name.clone())
                        .ok_or(RuntimeError::StateConflict)?
                };
                sources.push(AccountIoSource {
                    archive_account_id: ArchiveAccountId::new(slot.as_str().to_owned())
                        .map_err(model_error)?,
                    slot,
                    name,
                    ce_path,
                    de_path,
                });
            }
            Ok::<_, RuntimeError>(sources)
        })();
        let sources = match sources {
            Ok(sources) => sources,
            Err(error) => return Err(self.abandon_backup_lease(&intent, true, error)),
        };
        intent.phase = AccountIoPhase::Ready;
        if let Err(error) = self.packages.save_account_io_intent(&intent) {
            let error = logged_adapter("begin_backup_io", package, "save_ready", error);
            return Err(self.abandon_backup_lease(&intent, true, error));
        }
        Ok(AccountIoLease {
            io_token: token,
            package: package.clone(),
            kind: AccountIoKind::Backup,
            sources,
            staging: Vec::new(),
        })
    }

    pub fn finish_backup_io(&mut self, token: &AccountIoToken) -> Result<(), RuntimeError> {
        let intent = self.account_io_intent_by_token(token)?;
        if intent.kind != AccountIoKind::Backup {
            return Err(RuntimeError::StateConflict);
        }
        self.finish_backup_intent(&intent, false)
    }

    pub fn begin_restore_io(
        &mut self,
        package: &PackageName,
        transfer_id: TransferId,
        mappings: Vec<RestoreMapping>,
        archived_state: ArchivedState,
    ) -> Result<AccountIoLease, RuntimeError> {
        self.ensure_no_account_io(package)?;
        if mappings.is_empty() || mappings.len() > 256 {
            return Err(RuntimeError::InvalidRequest);
        }
        let (mut aggregate, inspection) = self.load_ready(package)?;
        let snapshot = aggregate.snapshot().map_err(model_error)?;
        validate_restore_mapping_rules(&mappings, &archived_state)?;
        let mut resolved = Vec::with_capacity(mappings.len());
        let mut aggregate_changed = false;
        for mapping in mappings {
            let (target_slot, create_slot, previous_name) = match &mapping.target {
                RestoreTarget::Existing { slot } => {
                    if !aggregate.has_slot(slot) {
                        return Err(RuntimeError::NotFound);
                    }
                    let previous_name = snapshot
                        .slots
                        .iter()
                        .find(|candidate| &candidate.id == slot)
                        .and_then(|candidate| {
                            (!slot.is_base())
                                .then(|| DisplayName::new(candidate.name.clone()).ok())
                                .flatten()
                        });
                    (slot.clone(), false, previous_name)
                }
                RestoreTarget::New => {
                    let target = aggregate
                        .reserve_restored_slot(mapping.name.clone())
                        .map_err(model_error)?;
                    aggregate_changed = true;
                    (target, true, None)
                }
            };
            resolved.push(ResolvedRestoreMapping {
                archive_account_id: mapping.archive_account_id,
                archive_kind: mapping.archive_kind,
                name: mapping.name,
                previous_name,
                target_slot,
                create_slot,
                restore_policy: mapping.restore_policy,
            });
        }
        let token = generate_account_io_token()?;
        let intent = AccountIoIntent {
            schema_version: 2,
            io_token: token.clone(),
            package: package.clone(),
            kind: AccountIoKind::Restore,
            scope: None,
            transfer_id: Some(transfer_id.clone()),
            mappings: resolved,
            archived_state: Some(archived_state),
            previous_active_slot: aggregate.active_slot().clone(),
            previous_launch_after_reboot: aggregate.launch_after_reboot(),
            previous_was_running: inspection.was_running,
            previous_enabled_state: None,
            maintenance_active: false,
            phase: AccountIoPhase::Ready,
            completed: Vec::new(),
        };
        self.packages
            .save_account_io_intent(&intent)
            .map_err(|error| logged_adapter("begin_restore_io", package, "save_intent", error))?;
        let account_ids = intent
            .mappings
            .iter()
            .map(|mapping| mapping.archive_account_id.clone())
            .collect::<Vec<_>>();
        let staging =
            match self
                .slots
                .prepare_restore_staging(package, &token, &transfer_id, &account_ids)
            {
                Ok(staging) => staging,
                Err(error) => {
                    let error =
                        logged_adapter("begin_restore_io", package, "prepare_staging", error);
                    return Err(self.abandon_restore_lease(package, &token, error));
                }
            };
        if aggregate_changed && let Err(error) = self.packages.save(&aggregate) {
            let error = logged_adapter("begin_restore_io", package, "reserve_target_slots", error);
            return Err(self.abandon_restore_lease(package, &token, error));
        }
        Ok(AccountIoLease {
            io_token: token,
            package: package.clone(),
            kind: AccountIoKind::Restore,
            sources: Vec::new(),
            staging,
        })
    }

    pub fn commit_restore_account(
        &mut self,
        token: &AccountIoToken,
        archive_account_id: &ArchiveAccountId,
    ) -> Result<RestoreItemResult, RuntimeError> {
        let mut intent = self.account_io_intent_by_token(token)?;
        if intent.kind != AccountIoKind::Restore {
            return Err(RuntimeError::StateConflict);
        }
        if let Some(result) = intent
            .completed
            .iter()
            .find(|result| &result.archive_account_id == archive_account_id)
        {
            return Ok(result.clone());
        }
        let mapping = intent
            .mappings
            .iter()
            .find(|mapping| &mapping.archive_account_id == archive_account_id)
            .cloned()
            .ok_or(RuntimeError::NotFound)?;
        let package = intent.package.clone();
        let transfer_id = intent
            .transfer_id
            .clone()
            .ok_or(RuntimeError::StateConflict)?;
        self.slots
            .validate_staged_account(
                &package,
                token,
                &transfer_id,
                archive_account_id,
                &mapping.restore_policy,
            )
            .map_err(|error| {
                logged_adapter(
                    "commit_restore_account",
                    &package,
                    "validate_staging",
                    error,
                )
            })?;
        let capacity_checked = !mapping.target_slot.is_base()
            || (!intent.maintenance_active && intent.previous_active_slot.is_base());
        if capacity_checked {
            self.slots
                .ensure_restore_capacity(
                    &package,
                    token,
                    &transfer_id,
                    archive_account_id,
                    &mapping.target_slot,
                    &mapping.restore_policy,
                )
                .map_err(|error| {
                    logged_adapter("commit_restore_account", &package, "check_capacity", error)
                })?;
        }
        if !intent.maintenance_active {
            let enabled = self.android.read_enabled_state(&package).map_err(|error| {
                logged_adapter(
                    "commit_restore_account",
                    &package,
                    "read_enabled_state",
                    error,
                )
            })?;
            intent.previous_enabled_state = Some(enabled);
            intent.maintenance_active = true;
            intent.phase = AccountIoPhase::Replacing {
                archive_account_id: archive_account_id.clone(),
                target_slot: mapping.target_slot.clone(),
            };
            self.packages
                .save_account_io_intent(&intent)
                .map_err(|error| {
                    logged_adapter(
                        "commit_restore_account",
                        &package,
                        "save_maintenance_intent",
                        error,
                    )
                })?;
            self.android.block_launch(&package).map_err(|error| {
                logged_adapter("commit_restore_account", &package, "block_launch", error)
            })?;
            self.stop_and_apply_view(&package, &SlotId::base())?;
            self.slots
                .prepare_maintenance(&package, token)
                .map_err(|error| {
                    logged_adapter(
                        "commit_restore_account",
                        &package,
                        "prepare_maintenance",
                        error,
                    )
                })?;
            self.android
                .apply_maintenance_view(&package, token)
                .map_err(|error| {
                    logged_adapter(
                        "commit_restore_account",
                        &package,
                        "apply_maintenance",
                        error,
                    )
                })?;
        } else {
            intent.phase = AccountIoPhase::Replacing {
                archive_account_id: archive_account_id.clone(),
                target_slot: mapping.target_slot.clone(),
            };
            self.packages
                .save_account_io_intent(&intent)
                .map_err(|error| {
                    logged_adapter(
                        "commit_restore_account",
                        &package,
                        "save_replace_intent",
                        error,
                    )
                })?;
        }

        if mapping.target_slot.is_base() {
            self.stop_and_apply_view(&package, &SlotId::base())?;
            if !capacity_checked {
                self.slots
                    .ensure_restore_capacity(
                        &package,
                        token,
                        &transfer_id,
                        archive_account_id,
                        &mapping.target_slot,
                        &mapping.restore_policy,
                    )
                    .map_err(|error| {
                        logged_adapter("commit_restore_account", &package, "check_capacity", error)
                    })?;
            }
        }

        if let Err(error) = self.slots.replace_account(
            &package,
            token,
            &transfer_id,
            archive_account_id,
            &mapping.target_slot,
            &mapping.restore_policy,
        ) {
            if error.is_insufficient_storage() {
                return Err(logged_adapter(
                    "commit_restore_account",
                    &package,
                    "replace_pair",
                    error,
                ));
            }
            log_adapter("commit_restore_account", &package, "replace_pair", &error);
            if let Err(rollback) = self.slots.rollback_account(
                &package,
                token,
                &mapping.target_slot,
                &mapping.restore_policy,
            ) {
                return Err(logged_conflict(
                    "commit_restore_account",
                    &package,
                    "rollback_pair",
                    rollback,
                ));
            }
            return self.record_restore_failure(intent, mapping);
        }

        let original = self
            .packages
            .load(&package)
            .map_err(adapter_error)?
            .ok_or(RuntimeError::NotFound)?;
        let mut aggregate = original.clone();
        let metadata_result = if mapping.create_slot {
            aggregate.install_restored_slot(&mapping.target_slot, mapping.name.clone())
        } else if mapping.target_slot.is_base() {
            Ok(())
        } else {
            aggregate.rename_slot(&mapping.target_slot, mapping.name.clone())
        };
        let metadata_failed = match metadata_result {
            Ok(()) => match self.packages.save(&aggregate) {
                Ok(()) => false,
                Err(error) => {
                    log_adapter("commit_restore_account", &package, "save_metadata", &error);
                    self.restore_original_metadata(&package, &original)?;
                    true
                }
            },
            Err(error) => {
                eprintln!(
                    "op=commit_restore_account package={package} step=apply_metadata error={error}"
                );
                true
            }
        };
        if metadata_failed {
            self.slots
                .rollback_account(
                    &package,
                    token,
                    &mapping.target_slot,
                    &mapping.restore_policy,
                )
                .map_err(|error| {
                    logged_conflict(
                        "commit_restore_account",
                        &package,
                        "rollback_after_metadata",
                        error,
                    )
                })?;
            return self.record_restore_failure(intent, mapping);
        }

        let result = RestoreItemResult {
            archive_account_id: archive_account_id.clone(),
            target_slot: Some(mapping.target_slot.clone()),
            state: RestoreItemState::Restored,
        };
        intent.completed.push(result.clone());
        intent.phase = AccountIoPhase::Ready;
        if let Err(error) = self.packages.save_account_io_intent(&intent) {
            log_adapter("commit_restore_account", &package, "save_success", &error);
            self.restore_original_metadata(&package, &original)?;
            self.slots
                .rollback_account(
                    &package,
                    token,
                    &mapping.target_slot,
                    &mapping.restore_policy,
                )
                .map_err(|rollback| {
                    logged_conflict(
                        "commit_restore_account",
                        &package,
                        "rollback_after_intent",
                        rollback,
                    )
                })?;
            return Err(adapter_error(error));
        }
        self.slots
            .finalize_account(&package, token, &mapping.target_slot)
            .map_err(|error| {
                logged_conflict(
                    "commit_restore_account",
                    &package,
                    "finalize_old_pair",
                    error,
                )
            })?;
        Ok(result)
    }

    pub fn finish_restore_io(
        &mut self,
        token: &AccountIoToken,
    ) -> Result<RestoreBatchResult, RuntimeError> {
        let mut intent = self.account_io_intent_by_token(token)?;
        if intent.kind != AccountIoKind::Restore {
            return Err(RuntimeError::StateConflict);
        }
        let package = intent.package.clone();
        add_skipped_restore_results(&mut intent);
        intent.phase = AccountIoPhase::Finalizing;
        self.packages
            .save_account_io_intent(&intent)
            .map_err(|error| {
                logged_adapter("finish_restore_io", &package, "save_finalizing", error)
            })?;
        self.finalize_restore_intent(intent)
    }

    pub fn abort_account_io(&mut self, token: &AccountIoToken) -> Result<(), RuntimeError> {
        let intent = self.account_io_intent_by_token(token)?;
        match intent.kind {
            AccountIoKind::Backup => self.finish_backup_intent(&intent, true),
            AccountIoKind::Restore => {
                self.abort_restore_intent(intent, true)?;
                Ok(())
            }
        }
    }

    pub fn list_account_io_status(&self) -> Result<Vec<AccountIoStatus>, RuntimeError> {
        self.packages
            .list_account_io_intents()
            .map(|intents| intents.into_iter().map(|intent| intent.status()).collect())
            .map_err(adapter_error)
    }

    pub fn enroll(
        &mut self,
        package: PackageName,
        signing: Option<SigningIdentity>,
    ) -> Result<PackageSnapshot, RuntimeError> {
        self.ensure_no_account_io(&package)?;
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
        self.ensure_no_account_io(package)?;
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
        self.ensure_no_account_io(package)?;
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
        if !aggregate.is_unenrolling() {
            aggregate.begin_unenrollment().map_err(model_error)?;
            self.packages
                .save(&aggregate)
                .map_err(|error| logged_adapter("unenroll", package, "save_intent", error))?;
        }
        self.finish_unenrollment(package, UnenrollContext::Request)?;
        self.purge_trash("unenroll");
        Ok(())
    }

    /// Deletes expired trash entries. A failure only delays the deletion to the next purge,
    /// so it is logged and never fails the operation that triggered it.
    fn purge_trash(&mut self, operation: &str) {
        let names = match self.packages.list() {
            Ok(names) => names,
            Err(error) => {
                eprintln!("op={operation} step=purge_trash_list error={error}");
                return;
            }
        };
        // A trash intent exists only while its unenroll is in progress, and the entry it names
        // stays until that unenroll completes. Older entries of the same package still expire;
        // only a package whose intent cannot be read keeps all of its trash.
        let held: Vec<_> = names
            .into_iter()
            .filter_map(|package| match self.packages.load_trash_intent(&package) {
                Ok(intent) => intent.map(|intent| TrashHold::Entry {
                    package: intent.package,
                    name: intent.name,
                }),
                Err(error) => {
                    eprintln!(
                        "op={operation} package={package} step=purge_trash_intent error={error}"
                    );
                    Some(TrashHold::Package(package))
                }
            })
            .collect();
        if let Err(error) = self.slots.purge_trash(&held) {
            eprintln!("op={operation} step=purge_trash error={error}");
        }
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
        self.slots
            .ensure_materialize_capacity(package, seed)
            .map_err(|error| logged_adapter("create_slot", package, "check_capacity", error))?;

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
        self.ensure_no_account_io(package)?;
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

    pub fn set_default_slot(
        &mut self,
        package: &PackageName,
        slot: Option<SlotId>,
    ) -> Result<PackageSnapshot, RuntimeError> {
        let (mut aggregate, _inspection) = self.load_ready(package)?;
        aggregate.set_default_slot(slot).map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("set_default_slot", package, "save", error))?;
        aggregate.snapshot().map_err(model_error)
    }

    /// Prepares the selected default without starting any App. The authenticated launch
    /// ticket owns the later foreground start; cancellation and rollback never relaunch it.
    pub fn prepare_default_account(
        &mut self,
        package: &PackageName,
        launch_permitted: impl FnMut() -> bool,
    ) -> Result<bool, RuntimeError> {
        let Some(aggregate) = self
            .packages
            .load(package)
            .map_err(|error| logged_adapter("prepare_default_account", package, "load", error))?
        else {
            return Ok(false);
        };
        aggregate.validate().map_err(model_error)?;
        let Some(target) = aggregate.default_slot().cloned() else {
            return Ok(false);
        };
        if aggregate.active_slot() == &target {
            return Ok(false);
        }
        let result = self.activate_slot_when(package, &target, launch_permitted, false)?;
        Ok(result.active_slot == target)
    }

    pub fn activate_slot(
        &mut self,
        package: &PackageName,
        target: &SlotId,
    ) -> Result<PackageSnapshot, RuntimeError> {
        self.activate_slot_when(package, target, || true, true)
    }

    fn activate_slot_when(
        &mut self,
        package: &PackageName,
        target: &SlotId,
        mut permitted: impl FnMut() -> bool,
        launch: bool,
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
        if !permitted() {
            return aggregate.snapshot().map_err(model_error);
        }
        if &previous == target {
            if launch {
                self.android
                    .launch_verified(package, target)
                    .map_err(|error| {
                        logged_adapter("activate_slot", package, "retry_launch", error)
                    })?;
            }
            return aggregate.snapshot().map_err(model_error);
        }

        aggregate.begin_activation(target).map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("activate_slot", package, "save_intent", error))?;
        if !permitted() {
            // The user left while preflight or journal persistence was in flight.
            // Clear only the intent: the app and its paired view have not been touched.
            aggregate.abort_activation(&previous).map_err(model_error)?;
            self.packages.save(&aggregate).map_err(|error| {
                logged_adapter("activate_slot", package, "cancel_departure", error)
            })?;
            return aggregate.snapshot().map_err(model_error);
        }
        if let Err(error) = self.android.force_stop(package) {
            log_adapter("activate_slot", package, "force_stop", &error);
            self.restore_after_failed_stop(
                &mut aggregate,
                &previous,
                inspection.was_running && launch && permitted(),
            )?;
            return Err(RuntimeError::OperationFailed);
        }
        if let Err(error) = self.android.apply_view(package, target) {
            log_adapter("activate_slot", package, "apply_target", &error);
            self.rollback_activation(
                &mut aggregate,
                &previous,
                inspection.was_running && launch && permitted(),
            )?;
            return Err(RuntimeError::OperationFailed);
        }
        let observed = self
            .android
            .observe_view(package)
            .map_err(|error| logged_adapter("activate_slot", package, "verify_target", error));
        if !matches!(observed, Ok(view) if view.matches(target)) {
            self.rollback_activation(
                &mut aggregate,
                &previous,
                inspection.was_running && launch && permitted(),
            )?;
            return Err(RuntimeError::OperationFailed);
        }

        aggregate.finish_activation(target).map_err(model_error)?;
        self.packages
            .save(&aggregate)
            .map_err(|error| logged_adapter("activate_slot", package, "save_commit", error))?;
        if launch {
            self.android
                .launch_verified(package, target)
                .map_err(|error| {
                    logged_adapter("activate_slot", package, "launch_target", error)
                })?;
        }
        aggregate.snapshot().map_err(model_error)
    }

    #[cfg(test)]
    pub(crate) fn into_parts(self) -> (P, S, A) {
        (self.packages, self.slots, self.android)
    }

    fn ensure_no_account_io(&self, package: &PackageName) -> Result<(), RuntimeError> {
        if self
            .packages
            .load_account_io_intent(package)
            .map_err(adapter_error)?
            .is_some()
        {
            Err(RuntimeError::IoBusy)
        } else {
            Ok(())
        }
    }

    fn account_io_intent_by_token(
        &self,
        token: &AccountIoToken,
    ) -> Result<AccountIoIntent, RuntimeError> {
        self.packages
            .list_account_io_intents()
            .map_err(adapter_error)?
            .into_iter()
            .find(|intent| &intent.io_token == token)
            .ok_or(RuntimeError::NotFound)
    }

    fn stop_and_apply_view(
        &mut self,
        package: &PackageName,
        target: &SlotId,
    ) -> Result<(), RuntimeError> {
        self.android
            .force_stop(package)
            .map_err(|error| logged_adapter("account_io", package, "force_stop", error))?;
        self.apply_view_verified(package, target)
    }

    fn apply_view_verified(
        &mut self,
        package: &PackageName,
        target: &SlotId,
    ) -> Result<(), RuntimeError> {
        self.android
            .apply_view(package, target)
            .map_err(|error| logged_adapter("account_io", package, "apply_view", error))?;
        let observed = self
            .android
            .observe_view(package)
            .map_err(|error| logged_adapter("account_io", package, "verify_view", error))?;
        if !observed.matches(target) {
            return Err(RuntimeError::StateConflict);
        }
        Ok(())
    }

    fn finish_backup_intent(
        &mut self,
        intent: &AccountIoIntent,
        relaunch: bool,
    ) -> Result<(), RuntimeError> {
        match self.release_backup_lease(intent, "finish_backup_io", relaunch, true)? {
            Relaunch::Failed => Err(RuntimeError::OperationFailed),
            Relaunch::NotFailed => Ok(()),
        }
    }

    /// Releases a backup lease: restores the previous view and enabled state, optionally
    /// relaunches the app, then removes staging and the intent. The intent is removed last,
    /// so any `Err` leaves it in place for abort or boot recovery to retry. A relaunch failure
    /// is logged and reported separately because the lease itself is gone by then.
    fn release_backup_lease(
        &mut self,
        intent: &AccountIoIntent,
        operation: &str,
        relaunch: bool,
        view_changed: bool,
    ) -> Result<Relaunch, RuntimeError> {
        let package = &intent.package;
        let mut relaunched = Relaunch::NotFailed;
        if intent.maintenance_active {
            if view_changed {
                self.stop_and_apply_view(package, &intent.previous_active_slot)?;
            }
            if let Some(enabled) = intent.previous_enabled_state {
                self.android
                    .restore_enabled_state(package, enabled)
                    .map_err(|error| {
                        logged_conflict(operation, package, "restore_enabled_state", error)
                    })?;
            }
            if relaunch
                && intent.previous_was_running
                && intent
                    .previous_enabled_state
                    .is_none_or(PackageEnabledState::allows_launch)
                && let Err(error) = self
                    .android
                    .launch_verified(package, &intent.previous_active_slot)
            {
                log_adapter(operation, package, "restore_launch", &error);
                relaunched = Relaunch::Failed;
            }
        }
        self.slots
            .cleanup_account_io(package, &intent.io_token)
            .map_err(|error| logged_conflict(operation, package, "cleanup", error))?;
        self.packages
            .clear_account_io_intent(package)
            .map_err(|error| logged_conflict(operation, package, "clear_intent", error))?;
        Ok(relaunched)
    }

    /// Undoes a backup lease whose setup failed with `error`. When the undo itself fails the
    /// intent stays persisted for abort or boot recovery, and the caller sees a conflict.
    fn abandon_backup_lease(
        &mut self,
        intent: &AccountIoIntent,
        view_changed: bool,
        error: RuntimeError,
    ) -> RuntimeError {
        match self.release_backup_lease(intent, "begin_backup_io", true, view_changed) {
            Ok(_relaunched) => error,
            Err(recovery) => {
                eprintln!(
                    "op=begin_backup_io package={} step=release_lease error={recovery} cause={error}",
                    intent.package
                );
                RuntimeError::StateConflict
            }
        }
    }

    /// Undoes a restore lease whose setup failed with `error`. Staging is removed before the
    /// intent, so a failed cleanup keeps the intent for abort or boot recovery to retry.
    fn abandon_restore_lease(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        error: RuntimeError,
    ) -> RuntimeError {
        if let Err(cleanup) = self.slots.cleanup_account_io(package, token) {
            log_adapter("begin_restore_io", package, "cleanup_staging", &cleanup);
            return RuntimeError::StateConflict;
        }
        if let Err(clear) = self.packages.clear_account_io_intent(package) {
            log_adapter("begin_restore_io", package, "clear_intent", &clear);
            return RuntimeError::StateConflict;
        }
        error
    }

    /// Puts back the aggregate saved before a restore commit. On failure the persisted
    /// `Replacing` intent still describes the pair and metadata, so abort or boot recovery
    /// rolls both back; the caller must not record the item as settled.
    fn restore_original_metadata(
        &mut self,
        package: &PackageName,
        original: &PackageAggregate,
    ) -> Result<(), RuntimeError> {
        self.packages.save(original).map_err(|error| {
            logged_conflict("commit_restore_account", package, "restore_metadata", error)
        })
    }

    fn record_restore_failure(
        &mut self,
        mut intent: AccountIoIntent,
        mapping: ResolvedRestoreMapping,
    ) -> Result<RestoreItemResult, RuntimeError> {
        let result = RestoreItemResult {
            archive_account_id: mapping.archive_account_id,
            target_slot: Some(mapping.target_slot),
            state: RestoreItemState::Failed,
        };
        intent.completed.push(result.clone());
        intent.phase = AccountIoPhase::Ready;
        self.packages
            .save_account_io_intent(&intent)
            .map_err(|error| {
                logged_conflict(
                    "commit_restore_account",
                    &intent.package,
                    "save_failure",
                    error,
                )
            })?;
        Ok(result)
    }

    fn rollback_restore_metadata(
        &mut self,
        intent: &AccountIoIntent,
        mapping: &ResolvedRestoreMapping,
    ) -> Result<(), RuntimeError> {
        let Some(mut aggregate) = self.packages.load(&intent.package).map_err(adapter_error)?
        else {
            return Err(RuntimeError::NotFound);
        };
        if mapping.create_slot {
            if aggregate.has_slot(&mapping.target_slot) {
                aggregate
                    .remove_restored_slot(&mapping.target_slot)
                    .map_err(model_error)?;
            }
        } else if !mapping.target_slot.is_base()
            && let Some(previous_name) = &mapping.previous_name
        {
            aggregate
                .rename_slot(&mapping.target_slot, previous_name.clone())
                .map_err(model_error)?;
        }
        self.packages.save(&aggregate).map_err(|error| {
            logged_conflict(
                "account_io_recovery",
                &intent.package,
                "rollback_metadata",
                error,
            )
        })
    }

    fn finalize_restore_intent(
        &mut self,
        intent: AccountIoIntent,
    ) -> Result<RestoreBatchResult, RuntimeError> {
        let package = intent.package.clone();
        let archived_state = intent
            .archived_state
            .clone()
            .ok_or(RuntimeError::StateConflict)?;
        let restored_active = archived_state
            .active_account_id
            .as_ref()
            .and_then(|active| {
                let restored = intent.completed.iter().any(|result| {
                    &result.archive_account_id == active
                        && result.state == RestoreItemState::Restored
                });
                restored.then(|| {
                    intent
                        .mappings
                        .iter()
                        .find(|mapping| &mapping.archive_account_id == active)
                        .map(|mapping| mapping.target_slot.clone())
                })?
            });
        let (target, launch_after_reboot) = if let Some(target) = restored_active {
            (target, archived_state.launch_after_reboot)
        } else {
            (
                intent.previous_active_slot.clone(),
                intent.previous_launch_after_reboot,
            )
        };

        self.stop_and_apply_view(&package, &target)?;
        let mut aggregate = self
            .packages
            .load(&package)
            .map_err(adapter_error)?
            .ok_or(RuntimeError::NotFound)?;
        aggregate
            .set_restored_active_slot(&target)
            .map_err(model_error)?;
        aggregate
            .set_launch_after_reboot(launch_after_reboot)
            .map_err(model_error)?;
        self.packages.save(&aggregate).map_err(|error| {
            logged_conflict("finish_restore_io", &package, "save_active", error)
        })?;

        if let Some(enabled) = intent.previous_enabled_state {
            self.android
                .restore_enabled_state(&package, enabled)
                .map_err(|error| {
                    logged_conflict(
                        "finish_restore_io",
                        &package,
                        "restore_enabled_state",
                        error,
                    )
                })?;
        }
        let result = RestoreBatchResult {
            package: package.clone(),
            items: intent.completed.clone(),
            active_slot: target,
            launch_after_reboot,
            app_started: false,
        };
        self.packages
            .save_account_io_result(&package, &result)
            .map_err(|error| {
                logged_conflict("finish_restore_io", &package, "save_result", error)
            })?;
        self.slots
            .cleanup_account_io(&package, &intent.io_token)
            .map_err(|error| logged_conflict("finish_restore_io", &package, "cleanup", error))?;
        self.packages
            .clear_account_io_intent(&package)
            .map_err(|error| {
                logged_conflict("finish_restore_io", &package, "clear_intent", error)
            })?;
        Ok(result)
    }

    fn abort_restore_intent(
        &mut self,
        mut intent: AccountIoIntent,
        relaunch: bool,
    ) -> Result<RestoreBatchResult, RuntimeError> {
        if intent.phase == AccountIoPhase::Finalizing {
            return self.finalize_restore_intent(intent);
        }
        let package = intent.package.clone();
        if let AccountIoPhase::Replacing {
            archive_account_id,
            target_slot,
        } = &intent.phase
        {
            let already_committed = intent.completed.iter().any(|result| {
                &result.archive_account_id == archive_account_id
                    && result.state == RestoreItemState::Restored
            });
            if !already_committed {
                let mapping = intent
                    .mappings
                    .iter()
                    .find(|mapping| &mapping.archive_account_id == archive_account_id)
                    .cloned()
                    .ok_or(RuntimeError::StateConflict)?;
                if target_slot.is_base() && intent.maintenance_active {
                    self.stop_and_apply_view(&package, &SlotId::base())?;
                }
                self.slots
                    .rollback_account(
                        &package,
                        &intent.io_token,
                        target_slot,
                        &mapping.restore_policy,
                    )
                    .map_err(|error| {
                        logged_conflict("abort_account_io", &package, "rollback_pair", error)
                    })?;
                self.rollback_restore_metadata(&intent, &mapping)?;
            }
        }
        add_skipped_restore_results(&mut intent);
        intent.phase = AccountIoPhase::Interrupted;
        self.packages
            .save_account_io_intent(&intent)
            .map_err(|error| {
                logged_conflict("abort_account_io", &package, "save_interrupted", error)
            })?;

        if intent.maintenance_active {
            self.stop_and_apply_view(&package, &intent.previous_active_slot)?;
        }
        let mut aggregate = self
            .packages
            .load(&package)
            .map_err(adapter_error)?
            .ok_or(RuntimeError::NotFound)?;
        aggregate
            .set_restored_active_slot(&intent.previous_active_slot)
            .map_err(model_error)?;
        aggregate
            .set_launch_after_reboot(intent.previous_launch_after_reboot)
            .map_err(model_error)?;
        self.packages.save(&aggregate).map_err(|error| {
            logged_conflict("abort_account_io", &package, "save_previous", error)
        })?;
        if let Some(enabled) = intent.previous_enabled_state {
            self.android
                .restore_enabled_state(&package, enabled)
                .map_err(|error| {
                    logged_conflict("abort_account_io", &package, "restore_enabled_state", error)
                })?;
        }
        let mut app_started = false;
        if relaunch
            && intent.maintenance_active
            && intent.previous_was_running
            && intent
                .previous_enabled_state
                .is_none_or(PackageEnabledState::allows_launch)
        {
            match self
                .android
                .launch_verified(&package, &intent.previous_active_slot)
            {
                Ok(()) => app_started = true,
                Err(error) => {
                    log_adapter("abort_account_io", &package, "restore_launch", &error);
                }
            }
        }
        let result = RestoreBatchResult {
            package: package.clone(),
            items: intent.completed.clone(),
            active_slot: intent.previous_active_slot.clone(),
            launch_after_reboot: intent.previous_launch_after_reboot,
            app_started,
        };
        self.packages
            .save_account_io_result(&package, &result)
            .map_err(|error| logged_conflict("abort_account_io", &package, "save_result", error))?;
        self.slots
            .cleanup_account_io(&package, &intent.io_token)
            .map_err(|error| logged_conflict("abort_account_io", &package, "cleanup", error))?;
        self.packages
            .clear_account_io_intent(&package)
            .map_err(|error| {
                logged_conflict("abort_account_io", &package, "clear_intent", error)
            })?;
        Ok(result)
    }

    fn recover_account_io_intents(&mut self) -> Result<(), RuntimeError> {
        let intents = self
            .packages
            .list_account_io_intents()
            .map_err(adapter_error)?;
        let mut outcome = Ok(());
        for intent in intents {
            let package = intent.package.clone();
            let recovery = match intent.kind {
                AccountIoKind::Backup => self.finish_backup_intent(&intent, false),
                AccountIoKind::Restore => {
                    self.abort_restore_intent(intent, false).map(|_result| ())
                }
            };
            if let Err(error) = recovery {
                eprintln!(
                    "op=reconcile_boot package={package} step=recover_account_io error={error}"
                );
                outcome = Err(RuntimeError::StateConflict);
            }
        }
        outcome
    }

    fn load_ready(
        &mut self,
        package: &PackageName,
    ) -> Result<(PackageAggregate, PackageInspection), RuntimeError> {
        self.ensure_no_account_io(package)?;
        let (aggregate, inspection) = self.load_validated(package)?;
        self.finish_ready_load(aggregate, inspection, ReadyLoadContext::Interactive)
    }

    fn finish_ready_load(
        &mut self,
        mut aggregate: PackageAggregate,
        inspection: PackageInspection,
        context: ReadyLoadContext,
    ) -> Result<(PackageAggregate, PackageInspection), RuntimeError> {
        let package = aggregate.package().clone();
        if aggregate.is_unenrolling() {
            self.finish_unenrollment(&package, UnenrollContext::Recovery)?;
            return Err(RuntimeError::NotFound);
        }
        let recovering_activation = aggregate.pending_activation().is_some();
        self.recover(&mut aggregate, context)?;
        if recovering_activation {
            return Err(RuntimeError::StateConflict);
        }
        self.converge_ready_view(&package, &aggregate, context)?;
        Ok((aggregate, inspection))
    }

    fn package_snapshot(&mut self, package: &PackageName) -> Result<PackageSnapshot, RuntimeError> {
        self.package_snapshot_with_context(package, ReadyLoadContext::Interactive)
    }

    fn package_snapshot_with_context(
        &mut self,
        package: &PackageName,
        context: ReadyLoadContext,
    ) -> Result<PackageSnapshot, RuntimeError> {
        let inspection = self
            .android
            .inspect(package)
            .map_err(|error| logged_adapter("load", package, "inspect", error))?;
        self.package_snapshot_with_inspection(package, inspection, context)
    }

    fn package_snapshot_with_inspection(
        &mut self,
        package: &PackageName,
        inspection: PackageInspection,
        context: ReadyLoadContext,
    ) -> Result<PackageSnapshot, RuntimeError> {
        let aggregate = self
            .packages
            .load(package)
            .map_err(adapter_error)?
            .ok_or(RuntimeError::NotFound)?;
        aggregate.validate().map_err(model_error)?;
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
        if self
            .packages
            .load_account_io_intent(package)
            .map_err(adapter_error)?
            .is_some()
        {
            return aggregate.snapshot_with_binding(state).map_err(model_error);
        }
        let (aggregate, _inspection) = self.finish_ready_load(aggregate, inspection, context)?;
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
        context: ReadyLoadContext,
    ) -> Result<(), RuntimeError> {
        let operation = context.operation();
        let observed = self
            .android
            .observe_view(package)
            .map_err(|error| logged_adapter(operation, package, "observe_ready", error))?;
        if !aggregate.is_ready() {
            return Err(RuntimeError::StateConflict);
        }
        if !observed.matches(aggregate.active_slot()) {
            if observed != ObservedView::Base || aggregate.active_slot().is_base() {
                return Err(RuntimeError::StateConflict);
            }
            self.slots
                .require_complete_pair(package, aggregate.active_slot())
                .map_err(|error| logged_conflict(operation, package, "verify_ready_slot", error))?;
            self.android.force_stop(package).map_err(|error| {
                logged_conflict(operation, package, "stop_for_ready_restore", error)
            })?;
            self.android
                .apply_view(package, aggregate.active_slot())
                .map_err(|error| {
                    logged_conflict(operation, package, "apply_ready_restore", error)
                })?;
            let restored = self.android.observe_view(package).map_err(|error| {
                logged_conflict(operation, package, "verify_ready_restore", error)
            })?;
            if !restored.matches(aggregate.active_slot()) {
                eprintln!(
                    "op={operation} package={package} step=verify_ready_restore error=view_not_restored"
                );
                return Err(RuntimeError::StateConflict);
            }
            if context == ReadyLoadContext::Boot && aggregate.launch_after_reboot() {
                self.android
                    .launch_verified(package, aggregate.active_slot())
                    .map_err(|error| {
                        logged_conflict(operation, package, "launch_ready_restore", error)
                    })?;
            }
        }
        Ok(())
    }

    fn recover(
        &mut self,
        aggregate: &mut PackageAggregate,
        context: ReadyLoadContext,
    ) -> Result<(), RuntimeError> {
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
            if restore_running && context == ReadyLoadContext::Interactive {
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
                if context == ReadyLoadContext::Interactive {
                    self.android
                        .launch_verified(&package, &target)
                        .map_err(|error| {
                            logged_conflict("recover", &package, "launch_target", error)
                        })?;
                }
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
                if context == ReadyLoadContext::Interactive {
                    self.android
                        .launch_verified(&package, &previous)
                        .map_err(|error| {
                            logged_conflict("recover", &package, "launch_previous", error)
                        })?;
                }
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
        let (ce, de) = self
            .slots
            .stage_package_trash(package)
            .map_err(|error| context.map_adapter(package, "stage_unenroll_trash", error))?;
        let intent = match self
            .packages
            .load_trash_intent(package)
            .map_err(|error| context.map_adapter(package, "load_unenroll_trash", error))?
        {
            Some(intent) => intent,
            None => {
                let name = self
                    .slots
                    .reserve_trash_name(package)
                    .map_err(|error| context.map_adapter(package, "name_unenroll_trash", error))?;
                let intent = TrashIntent {
                    package: package.clone(),
                    name,
                    ce,
                    de,
                };
                self.packages
                    .save_trash_intent(&intent)
                    .map_err(|error| context.map_adapter(package, "save_unenroll_trash", error))?;
                intent
            }
        };
        self.slots
            .finalize_package_trash(&intent)
            .map_err(|error| context.map_adapter(package, "finalize_unenroll_trash", error))?;
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

fn generate_account_io_token() -> Result<AccountIoToken, RuntimeError> {
    let mut bytes = [0_u8; 16];
    File::open("/dev/urandom")
        .and_then(|mut random| random.read_exact(&mut bytes))
        .map_err(|error| {
            eprintln!("op=account_io step=generate_token error={error}");
            RuntimeError::OperationFailed
        })?;
    Ok(AccountIoToken::from_bytes(bytes))
}

fn validate_restore_mapping_rules(
    mappings: &[RestoreMapping],
    archived_state: &ArchivedState,
) -> Result<(), RuntimeError> {
    let source_ids = mappings
        .iter()
        .map(|mapping| &mapping.archive_account_id)
        .collect::<BTreeSet<_>>();
    if source_ids.len() != mappings.len() {
        return Err(RuntimeError::InvalidRequest);
    }
    if mappings
        .iter()
        .any(|mapping| mapping.restore_policy.validate().is_err())
    {
        return Err(RuntimeError::InvalidRequest);
    }
    let existing_targets = mappings
        .iter()
        .filter_map(|mapping| match &mapping.target {
            RestoreTarget::Existing { slot } => Some(slot),
            RestoreTarget::New => None,
        })
        .collect::<BTreeSet<_>>();
    let existing_count = mappings
        .iter()
        .filter(|mapping| matches!(mapping.target, RestoreTarget::Existing { .. }))
        .count();
    if existing_targets.len() != existing_count {
        return Err(RuntimeError::InvalidRequest);
    }
    if let Some(active) = &archived_state.active_account_id
        && !source_ids.contains(active)
    {
        return Err(RuntimeError::InvalidRequest);
    }
    if mappings.iter().any(|mapping| match &mapping.target {
        RestoreTarget::Existing { slot } => {
            (mapping.archive_kind == ArchiveAccountKind::Base) != slot.is_base()
        }
        RestoreTarget::New => mapping.archive_kind == ArchiveAccountKind::Base,
    }) {
        return Err(RuntimeError::InvalidRequest);
    }
    match archived_state.scope {
        ArchiveScope::Account => {
            if mappings.len() != 1 {
                return Err(RuntimeError::InvalidRequest);
            }
        }
        ArchiveScope::AllAccounts => {
            let base_mappings = mappings
                .iter()
                .filter(|mapping| mapping.archive_kind == ArchiveAccountKind::Base)
                .count();
            if base_mappings != 1 {
                return Err(RuntimeError::InvalidRequest);
            }
        }
    }
    Ok(())
}

fn add_skipped_restore_results(intent: &mut AccountIoIntent) {
    let completed = intent
        .completed
        .iter()
        .map(|result| result.archive_account_id.clone())
        .collect::<BTreeSet<_>>();
    for mapping in &intent.mappings {
        if !completed.contains(&mapping.archive_account_id) {
            intent.completed.push(RestoreItemResult {
                archive_account_id: mapping.archive_account_id.clone(),
                target_slot: Some(mapping.target_slot.clone()),
                state: RestoreItemState::Skipped,
            });
        }
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
    if error.is_insufficient_storage() {
        RuntimeError::InsufficientStorage
    } else if error.is_state_conflict() {
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
mod tests;
