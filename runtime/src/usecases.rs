use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read as _;

use thiserror::Error;

use crate::model::{
    AccountIoIntent, AccountIoKind, AccountIoLease, AccountIoPhase, AccountIoScope,
    AccountIoSource, AccountIoStatus, AccountIoToken, ArchiveAccountId, ArchiveScope,
    ArchivedAccountKind, ArchivedState, BindingState, Capabilities, DisplayName, ModelError,
    ObservedView, PackageAggregate, PackageBinding, PackageEnabledState, PackageInspection,
    PackageName, PackageSnapshot, RebindIntent, ResolvedRestoreMapping, RestoreBatchResult,
    RestoreItemResult, RestoreItemState, RestoreMapping, RestoreTarget, SeedMode, SigningIdentity,
    SlotId, TransferId,
};
#[cfg(test)]
use crate::model::{RestoreDomain, RestorePath, RestorePolicy};
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
    #[error("backup is invalid")]
    BackupInvalid,
    #[error("backup password is required")]
    BackupPasswordRequired,
    #[error("backup authentication failed")]
    BackupAuthFailed,
    #[error("insufficient storage")]
    InsufficientStorage,
    #[error("backup is incompatible")]
    BackupIncompatible,
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

    pub fn reconcile_boot(&mut self) -> Result<(), RuntimeError> {
        self.recover_account_io_intents()?;
        let names = self.packages.list().map_err(adapter_error)?;
        for package in names {
            if let Err(error) = self.package_snapshot_with_context(&package, ReadyLoadContext::Boot)
            {
                eprintln!(
                    "op=reconcile_boot package={package} step=reconcile_package error={error}"
                );
            }
        }
        Ok(())
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
                let _recovery = self.finish_backup_before_view_change(&intent, true);
                return Err(error);
            }
            if let Err(error) = self.apply_view_verified(package, &SlotId::base()) {
                let _recovery = self.finish_backup_intent(&intent, true);
                return Err(error);
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
                    "系统原始空间".to_owned()
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
            Err(error) => {
                let _recovery = self.finish_backup_intent(&intent, true);
                return Err(error);
            }
        };
        intent.phase = AccountIoPhase::Ready;
        if let Err(error) = self.packages.save_account_io_intent(&intent) {
            let error = logged_adapter("begin_backup_io", package, "save_ready", error);
            let _recovery = self.finish_backup_intent(&intent, true);
            return Err(error);
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
                    let _cleanup = self.slots.cleanup_account_io(package, &token);
                    let _clear = self.packages.clear_account_io_intent(package);
                    return Err(logged_adapter(
                        "begin_restore_io",
                        package,
                        "prepare_staging",
                        error,
                    ));
                }
            };
        if aggregate_changed && let Err(error) = self.packages.save(&aggregate) {
            let _cleanup = self.slots.cleanup_account_io(package, &token);
            let _clear = self.packages.clear_account_io_intent(package);
            return Err(logged_adapter(
                "begin_restore_io",
                package,
                "reserve_target_slots",
                error,
            ));
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
        if metadata_result.is_err() || self.packages.save(&aggregate).is_err() {
            let _restore_metadata = self.packages.save(&original);
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
            let _restore_metadata = self.packages.save(&original);
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
            return Err(logged_adapter(
                "commit_restore_account",
                &package,
                "save_success",
                error,
            ));
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
        reset: bool,
        signing: Option<SigningIdentity>,
    ) -> Result<PackageSnapshot, RuntimeError> {
        self.ensure_no_account_io(&package)?;
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
        let package = &intent.package;
        let mut launch_error = None;
        if intent.maintenance_active {
            self.stop_and_apply_view(package, &intent.previous_active_slot)?;
            if let Some(enabled) = intent.previous_enabled_state {
                self.android
                    .restore_enabled_state(package, enabled)
                    .map_err(|error| {
                        logged_conflict("finish_backup_io", package, "restore_enabled_state", error)
                    })?;
            }
            let enabled_allows_launch = !matches!(
                intent.previous_enabled_state,
                Some(
                    PackageEnabledState::Disabled
                        | PackageEnabledState::DisabledUser
                        | PackageEnabledState::DisabledUntilUsed
                )
            );
            if relaunch
                && intent.previous_was_running
                && enabled_allows_launch
                && let Err(error) = self
                    .android
                    .launch_verified(package, &intent.previous_active_slot)
            {
                log_adapter("finish_backup_io", package, "restore_launch", &error);
                launch_error = Some(RuntimeError::OperationFailed);
            }
        }
        self.slots
            .cleanup_account_io(package, &intent.io_token)
            .map_err(|error| logged_conflict("finish_backup_io", package, "cleanup", error))?;
        self.packages
            .clear_account_io_intent(package)
            .map_err(|error| logged_conflict("finish_backup_io", package, "clear_intent", error))?;
        launch_error.map_or(Ok(()), Err)
    }

    fn finish_backup_before_view_change(
        &mut self,
        intent: &AccountIoIntent,
        relaunch: bool,
    ) -> Result<(), RuntimeError> {
        let package = &intent.package;
        let mut launch_error = None;
        if let Some(enabled) = intent.previous_enabled_state {
            self.android
                .restore_enabled_state(package, enabled)
                .map_err(|error| {
                    logged_conflict("begin_backup_io", package, "restore_enabled_state", error)
                })?;
        }
        let enabled_allows_launch = !matches!(
            intent.previous_enabled_state,
            Some(
                PackageEnabledState::Disabled
                    | PackageEnabledState::DisabledUser
                    | PackageEnabledState::DisabledUntilUsed
            )
        );
        if relaunch
            && intent.previous_was_running
            && enabled_allows_launch
            && let Err(error) = self
                .android
                .launch_verified(package, &intent.previous_active_slot)
        {
            log_adapter("begin_backup_io", package, "restore_launch", &error);
            launch_error = Some(RuntimeError::OperationFailed);
        }
        self.slots
            .cleanup_account_io(package, &intent.io_token)
            .map_err(|error| logged_conflict("begin_backup_io", package, "cleanup", error))?;
        self.packages
            .clear_account_io_intent(package)
            .map_err(|error| logged_conflict("begin_backup_io", package, "clear_intent", error))?;
        launch_error.map_or(Ok(()), Err)
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
        let enabled_allows_launch = !matches!(
            intent.previous_enabled_state,
            Some(
                PackageEnabledState::Disabled
                    | PackageEnabledState::DisabledUser
                    | PackageEnabledState::DisabledUntilUsed
            )
        );
        if relaunch
            && intent.maintenance_active
            && intent.previous_was_running
            && enabled_allows_launch
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
            }
        }
        Ok(())
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
            (mapping.archive_kind == ArchivedAccountKind::Base) != slot.is_base()
        }
        RestoreTarget::New => mapping.archive_kind == ArchivedAccountKind::Base,
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
                .filter(|mapping| mapping.archive_kind == ArchivedAccountKind::Base)
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
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::adapters::{
        AndroidCall, AndroidFailure, FilePackageStore, MemoryAndroidOps, MemoryPackageStore,
        MemorySlotStorage, SlotFailure,
    };

    type TestRuntime = Runtime<MemoryPackageStore, MemorySlotStorage, MemoryAndroidOps>;

    #[derive(Debug, Default)]
    struct CapacityDeniedStorage(MemorySlotStorage);

    impl SlotStorage for CapacityDeniedStorage {
        fn materialize(
            &mut self,
            package: &PackageName,
            slot: &crate::model::Slot,
            seed: SeedMode,
        ) -> Result<(), AdapterError> {
            self.0.materialize(package, slot, seed)
        }

        fn discard(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError> {
            self.0.discard(package, slot)
        }

        fn discard_package(&mut self, package: &PackageName) -> Result<(), AdapterError> {
            self.0.discard_package(package)
        }

        fn require_complete_pair(
            &self,
            package: &PackageName,
            slot: &SlotId,
        ) -> Result<(), AdapterError> {
            self.0.require_complete_pair(package, slot)
        }

        fn account_paths(
            &self,
            package: &PackageName,
            slot: &SlotId,
        ) -> Result<(String, String), AdapterError> {
            self.0.account_paths(package, slot)
        }

        fn prepare_restore_staging(
            &mut self,
            package: &PackageName,
            token: &AccountIoToken,
            transfer_id: &TransferId,
            accounts: &[ArchiveAccountId],
        ) -> Result<Vec<crate::model::RestoreStaging>, AdapterError> {
            self.0
                .prepare_restore_staging(package, token, transfer_id, accounts)
        }

        fn validate_staged_account(
            &self,
            package: &PackageName,
            token: &AccountIoToken,
            transfer_id: &TransferId,
            account: &ArchiveAccountId,
            policy: &RestorePolicy,
        ) -> Result<(), AdapterError> {
            self.0
                .validate_staged_account(package, token, transfer_id, account, policy)
        }

        fn ensure_restore_capacity(
            &self,
            _package: &PackageName,
            _token: &AccountIoToken,
            _transfer_id: &TransferId,
            _account: &ArchiveAccountId,
            target: &SlotId,
            _policy: &RestorePolicy,
        ) -> Result<(), AdapterError> {
            if target.is_base() {
                Err(AdapterError::insufficient_storage(
                    "injected insufficient Base rollback capacity",
                ))
            } else {
                Ok(())
            }
        }

        fn prepare_maintenance(
            &mut self,
            package: &PackageName,
            token: &AccountIoToken,
        ) -> Result<(), AdapterError> {
            self.0.prepare_maintenance(package, token)
        }

        fn replace_account(
            &mut self,
            package: &PackageName,
            token: &AccountIoToken,
            transfer_id: &TransferId,
            account: &ArchiveAccountId,
            target: &SlotId,
            policy: &RestorePolicy,
        ) -> Result<(), AdapterError> {
            self.0
                .replace_account(package, token, transfer_id, account, target, policy)
        }

        fn rollback_account(
            &mut self,
            package: &PackageName,
            token: &AccountIoToken,
            target: &SlotId,
            policy: &RestorePolicy,
        ) -> Result<(), AdapterError> {
            self.0.rollback_account(package, token, target, policy)
        }

        fn finalize_account(
            &mut self,
            package: &PackageName,
            token: &AccountIoToken,
            target: &SlotId,
        ) -> Result<(), AdapterError> {
            self.0.finalize_account(package, token, target)
        }

        fn cleanup_account_io(
            &mut self,
            package: &PackageName,
            token: &AccountIoToken,
        ) -> Result<(), AdapterError> {
            self.0.cleanup_account_io(package, token)
        }
    }

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
    fn boot_reconcile_never_relaunches_interrupted_activation() {
        let package = package();
        let (runtime, target) = with_slot();
        let (mut packages, slots, mut android) = runtime.into_parts();
        let mut aggregate = packages.load(&package).unwrap().unwrap();
        aggregate.begin_activation(&target).unwrap();
        packages.save(&aggregate).unwrap();
        android.force_stop(&package).unwrap();
        android.apply_view(&package, &target).unwrap();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        runtime.reconcile_boot().unwrap();

        let (packages, _slots, android) = runtime.into_parts();
        let aggregate = packages.load(&package).unwrap().unwrap();
        assert_eq!(aggregate.active_slot(), &target);
        assert!(aggregate.is_ready());
        assert!(!android.is_running(&package));
        assert!(
            android.calls()[starting_calls..]
                .iter()
                .all(|call| !matches!(call, AndroidCall::Launch(..)))
        );
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
    fn interactive_restore_never_uses_the_reboot_launch_setting() {
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
    fn package_listing_restores_views_without_launching_apps() {
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
        assert!(!android.is_running(&opted_in));
        assert!(!android.is_running(&opted_out));
        for (package, target) in targets {
            assert_eq!(android.view(&package), Some(&ObservedView::Slot(target)));
        }
    }

    #[test]
    fn boot_reconcile_launches_only_opted_in_non_base_packages() {
        let opted_in = PackageName::new("com.example.opted.in").unwrap();
        let opted_out = PackageName::new("com.example.opted.out").unwrap();
        let base = PackageName::new("com.example.base").unwrap();
        let mut android = MemoryAndroidOps::default();
        for package in [&opted_in, &opted_out, &base] {
            android.install(package.clone());
        }
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
        runtime.enroll(base.clone(), false, None).unwrap();
        runtime.set_launch_after_reboot(&opted_in, true).unwrap();
        runtime.set_launch_after_reboot(&base, true).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        for package in [&opted_in, &opted_out, &base] {
            android.force_stop(package).unwrap();
        }
        for (package, _) in &targets {
            android.set_view(package, ObservedView::Base);
        }
        let mut runtime = Runtime::new(packages, slots, android);

        runtime.reconcile_boot().unwrap();

        let (_packages, _slots, android) = runtime.into_parts();
        assert!(android.is_running(&opted_in));
        assert!(!android.is_running(&opted_out));
        assert!(!android.is_running(&base));
        for (package, target) in targets {
            assert_eq!(android.view(&package), Some(&ObservedView::Slot(target)));
        }
    }

    #[test]
    fn boot_reconcile_continues_after_one_package_fails() {
        let first = PackageName::new("com.example.a").unwrap();
        let second = PackageName::new("com.example.b").unwrap();
        let mut android = MemoryAndroidOps::default();
        android.install(first.clone());
        android.install(second.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            android,
        );
        let mut targets = Vec::new();
        for package in [&first, &second] {
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
        let (packages, slots, mut android) = runtime.into_parts();
        for (package, _) in &targets {
            android.force_stop(package).unwrap();
            android.set_view(package, ObservedView::Base);
        }
        android.fail_next(AndroidFailure::Observe);
        let mut runtime = Runtime::new(packages, slots, android);

        runtime.reconcile_boot().unwrap();

        let (_packages, _slots, android) = runtime.into_parts();
        assert_eq!(android.view(&first), Some(&ObservedView::Base));
        assert_eq!(
            android.view(&second),
            Some(&ObservedView::Slot(targets[1].1.clone()))
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

    #[test]
    fn inactive_account_backup_holds_a_package_lease_without_stopping_the_app() {
        let package = package();
        let (mut runtime, inactive) = with_slot();
        let starting_calls = runtime.android.calls().len();
        let lease = runtime
            .begin_backup_io(
                &package,
                AccountIoScope::Account {
                    slot: inactive.clone(),
                },
            )
            .unwrap();

        assert_eq!(
            runtime.rename_slot(&package, &inactive, DisplayName::new("Busy").unwrap()),
            Err(RuntimeError::IoBusy),
        );
        runtime.finish_backup_io(&lease.io_token).unwrap();

        let (_packages, _slots, android) = runtime.into_parts();
        assert!(android.is_running(&package));
        assert!(
            !android.calls()[starting_calls..]
                .iter()
                .any(|call| matches!(call, AndroidCall::ForceStop(_)))
        );
    }

    #[test]
    fn full_backup_restores_the_running_view_without_relaunching_the_app() {
        let package = package();
        let (mut runtime, active) = with_slot();
        runtime.activate_slot(&package, &active).unwrap();

        let lease = runtime
            .begin_backup_io(&package, AccountIoScope::AllAccounts)
            .unwrap();
        assert!(!runtime.android.is_running(&package));
        assert_eq!(
            runtime.android.enabled_state(&package),
            Some(PackageEnabledState::DisabledUser),
        );
        let status = runtime.list_account_io_status().unwrap();
        assert_eq!(status[0].phase, AccountIoPhase::Ready);

        runtime.finish_backup_io(&lease.io_token).unwrap();
        let (_packages, _slots, android) = runtime.into_parts();
        assert_eq!(android.view(&package), Some(&ObservedView::Slot(active)));
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::Default),
        );
        assert!(!android.is_running(&package));
    }

    #[test]
    fn active_base_backup_restores_base_without_relaunching_the_app() {
        let package = package();
        let (mut runtime, _inactive) = with_slot();

        let lease = runtime
            .begin_backup_io(
                &package,
                AccountIoScope::Account {
                    slot: SlotId::base(),
                },
            )
            .unwrap();
        assert!(!runtime.android.is_running(&package));

        runtime.finish_backup_io(&lease.io_token).unwrap();
        let (_packages, _slots, android) = runtime.into_parts();
        assert_eq!(android.view(&package), Some(&ObservedView::Base));
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::Default),
        );
        assert!(!android.is_running(&package));
    }

    #[test]
    fn active_non_base_backup_restores_the_slot_without_relaunching_the_app() {
        let package = package();
        let (mut runtime, active) = with_slot();
        runtime.activate_slot(&package, &active).unwrap();

        let lease = runtime
            .begin_backup_io(
                &package,
                AccountIoScope::Account {
                    slot: active.clone(),
                },
            )
            .unwrap();
        assert!(!runtime.android.is_running(&package));

        runtime.finish_backup_io(&lease.io_token).unwrap();
        let (_packages, _slots, android) = runtime.into_parts();
        assert_eq!(android.view(&package), Some(&ObservedView::Slot(active)));
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::Default),
        );
        assert!(!android.is_running(&package));
    }

    #[test]
    fn active_backup_does_not_force_stop_twice_after_launch_is_blocked() {
        let package = package();
        let (mut runtime, active) = with_slot();
        runtime.activate_slot(&package, &active).unwrap();
        let starting_calls = runtime.android.calls().len();

        let lease = runtime
            .begin_backup_io(
                &package,
                AccountIoScope::Account {
                    slot: active.clone(),
                },
            )
            .unwrap();

        let calls = &runtime.android.calls()[starting_calls..];
        let launch_blocked = calls
            .iter()
            .position(|call| matches!(call, AndroidCall::BlockLaunch(value) if value == &package))
            .unwrap();
        assert!(
            calls[launch_blocked + 1..]
                .iter()
                .all(|call| !matches!(call, AndroidCall::ForceStop(_)))
        );
        runtime.abort_account_io(&lease.io_token).unwrap();
    }

    #[test]
    fn backup_restores_a_previously_disabled_package_without_launching_it() {
        let package = package();
        let (mut runtime, _active) = with_slot();
        runtime
            .android
            .set_enabled_state(&package, PackageEnabledState::DisabledUser);

        let lease = runtime
            .begin_backup_io(&package, AccountIoScope::AllAccounts)
            .unwrap();
        runtime.finish_backup_io(&lease.io_token).unwrap();

        let (_packages, _slots, android) = runtime.into_parts();
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::DisabledUser),
        );
        assert!(!android.is_running(&package));
    }

    #[test]
    fn partial_launch_block_failure_restores_the_pre_backup_state() {
        let package = package();
        let (mut runtime, active) = with_slot();
        runtime.activate_slot(&package, &active).unwrap();
        runtime.android.fail_next(AndroidFailure::BlockLaunch);

        assert_eq!(
            runtime.begin_backup_io(&package, AccountIoScope::AllAccounts),
            Err(RuntimeError::OperationFailed),
        );

        let (packages, _slots, android) = runtime.into_parts();
        assert!(packages.load_account_io_intent(&package).unwrap().is_none());
        assert_eq!(android.view(&package), Some(&ObservedView::Slot(active)));
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::Default),
        );
        assert!(android.is_running(&package));
    }

    #[test]
    fn launch_block_failure_does_not_need_a_second_force_stop_to_release_the_lease() {
        let package = package();
        let (mut runtime, active) = with_slot();
        runtime.activate_slot(&package, &active).unwrap();
        runtime.android.fail_next(AndroidFailure::BlockLaunch);
        runtime.android.fail_next(AndroidFailure::ForceStop);

        assert_eq!(
            runtime.begin_backup_io(&package, AccountIoScope::AllAccounts),
            Err(RuntimeError::OperationFailed),
        );

        let (packages, _slots, android) = runtime.into_parts();
        assert!(packages.load_account_io_intent(&package).unwrap().is_none());
        assert_eq!(android.view(&package), Some(&ObservedView::Slot(active)));
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::Default),
        );
        assert!(android.is_running(&package));
    }

    #[test]
    fn launch_block_failure_clears_the_lease_even_if_relaunch_fails() {
        let package = package();
        let (mut runtime, active) = with_slot();
        runtime.activate_slot(&package, &active).unwrap();
        runtime.android.fail_next(AndroidFailure::BlockLaunch);
        runtime.android.fail_next(AndroidFailure::Launch);

        assert_eq!(
            runtime.begin_backup_io(&package, AccountIoScope::AllAccounts),
            Err(RuntimeError::OperationFailed),
        );

        let (packages, _slots, android) = runtime.into_parts();
        assert!(packages.load_account_io_intent(&package).unwrap().is_none());
        assert_eq!(android.view(&package), Some(&ObservedView::Slot(active)));
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::Default),
        );
        assert!(!android.is_running(&package));
    }

    #[test]
    fn launch_block_failure_keeps_a_recoverable_intent_when_enabled_state_cannot_be_restored() {
        let package = package();
        let (mut runtime, active) = with_slot();
        runtime.activate_slot(&package, &active).unwrap();
        runtime.android.fail_next(AndroidFailure::BlockLaunch);
        runtime.android.fail_next(AndroidFailure::RestoreEnabled);

        assert_eq!(
            runtime.begin_backup_io(&package, AccountIoScope::AllAccounts),
            Err(RuntimeError::OperationFailed),
        );

        let (packages, slots, android) = runtime.into_parts();
        assert!(packages.load_account_io_intent(&package).unwrap().is_some());
        assert_eq!(
            android.view(&package),
            Some(&ObservedView::Slot(active.clone()))
        );
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::DisabledUser),
        );
        assert!(!android.is_running(&package));

        let mut runtime = Runtime::new(packages, slots, android);
        runtime.reconcile_boot().unwrap();

        let (packages, _slots, android) = runtime.into_parts();
        assert!(packages.load_account_io_intent(&package).unwrap().is_none());
        assert_eq!(android.view(&package), Some(&ObservedView::Slot(active)));
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::Default),
        );
        assert!(!android.is_running(&package));
    }

    #[test]
    fn boot_recovers_an_interrupted_backup_before_normal_slot_convergence_without_launching() {
        let package = package();
        let (mut runtime, active) = with_slot();
        runtime.activate_slot(&package, &active).unwrap();
        runtime
            .begin_backup_io(&package, AccountIoScope::AllAccounts)
            .unwrap();
        let (packages, slots, android) = runtime.into_parts();
        let mut runtime = Runtime::new(packages, slots, android);

        runtime.reconcile_boot().unwrap();

        let (_packages, _slots, android) = runtime.into_parts();
        assert_eq!(android.view(&package), Some(&ObservedView::Slot(active)));
        assert_eq!(
            android.enabled_state(&package),
            Some(PackageEnabledState::Default),
        );
        assert!(!android.is_running(&package));
    }

    #[test]
    fn restore_blocks_launch_before_exposing_base_or_maintenance_views() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        let archive_id = ArchiveAccountId::new("account-0").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-order").unwrap(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Base,
                    name: DisplayName::new("Base").unwrap(),
                    target: RestoreTarget::Existing {
                        slot: SlotId::base(),
                    },
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id.clone()),
                    launch_after_reboot: false,
                },
            )
            .unwrap();
        let first_commit_call = runtime.android.calls().len();

        runtime
            .commit_restore_account(&lease.io_token, &archive_id)
            .unwrap();

        let calls = &runtime.android.calls()[first_commit_call..];
        assert!(
            matches!(calls.first(), Some(AndroidCall::BlockLaunch(value)) if value == &package)
        );
        let base_view = calls
            .iter()
            .position(|call| matches!(call, AndroidCall::Apply(_, slot) if slot.is_base()))
            .unwrap();
        let maintenance = calls
            .iter()
            .position(|call| matches!(call, AndroidCall::Maintenance(_, _)))
            .unwrap();
        assert!(base_view > 0);
        assert!(maintenance > base_view);
    }

    #[test]
    fn restore_policy_is_persisted_in_schema_two_while_schema_one_defaults_to_replace() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        let archive_id = ArchiveAccountId::new("account-policy").unwrap();
        let policy = RestorePolicy::PreservePaths {
            paths: vec![RestorePath {
                domain: RestoreDomain::Ce,
                path: "files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer".to_owned(),
            }],
        };

        runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-policy").unwrap(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Base,
                    name: DisplayName::new("Base").unwrap(),
                    target: RestoreTarget::Existing {
                        slot: SlotId::base(),
                    },
                    restore_policy: policy.clone(),
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id),
                    launch_after_reboot: false,
                },
            )
            .unwrap();

        let intent = runtime
            .packages
            .load_account_io_intent(&package)
            .unwrap()
            .unwrap();
        assert_eq!(intent.schema_version, 2);
        assert_eq!(intent.mappings[0].restore_policy, policy);

        let mut legacy = serde_json::to_value(intent).unwrap();
        legacy["schema_version"] = serde_json::json!(1);
        legacy["mappings"][0]
            .as_object_mut()
            .unwrap()
            .remove("restore_policy");
        let legacy: AccountIoIntent = serde_json::from_value(legacy).unwrap();
        assert_eq!(legacy.mappings[0].restore_policy, RestorePolicy::Replace);
        assert!(legacy.validate().is_ok());
    }

    #[test]
    fn insufficient_base_rollback_capacity_preserves_ready_intent_and_staging() {
        let package = package();
        let mut android = MemoryAndroidOps::default();
        android.install(package.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            CapacityDeniedStorage::default(),
            android,
        );
        runtime.enroll(package.clone(), false, None).unwrap();
        let archive_id = ArchiveAccountId::new("account-capacity").unwrap();
        let transfer_id = TransferId::new("transfer-capacity").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                transfer_id.clone(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Base,
                    name: DisplayName::new("Base").unwrap(),
                    target: RestoreTarget::Existing {
                        slot: SlotId::base(),
                    },
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id.clone()),
                    launch_after_reboot: false,
                },
            )
            .unwrap();
        let intent_before = runtime
            .packages
            .load_account_io_intent(&package)
            .unwrap()
            .unwrap();
        let calls_before = runtime.android.calls().to_vec();

        assert_eq!(
            runtime.commit_restore_account(&lease.io_token, &archive_id),
            Err(RuntimeError::InsufficientStorage),
        );

        assert_eq!(
            runtime
                .packages
                .load_account_io_intent(&package)
                .unwrap()
                .unwrap(),
            intent_before,
        );
        assert_eq!(runtime.android.calls(), calls_before);
        runtime
            .slots
            .validate_staged_account(
                &package,
                &lease.io_token,
                &transfer_id,
                &archive_id,
                &RestorePolicy::Replace,
            )
            .unwrap();
        assert_eq!(
            runtime.commit_restore_account(&lease.io_token, &archive_id),
            Err(RuntimeError::InsufficientStorage),
        );
    }

    #[test]
    fn insufficient_base_capacity_from_an_active_slot_can_abort_to_the_previous_view() {
        let package = package();
        let mut android = MemoryAndroidOps::default();
        android.install(package.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            CapacityDeniedStorage::default(),
            android,
        );
        runtime.enroll(package.clone(), false, None).unwrap();
        let created = runtime
            .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap();
        let previous = created
            .slots
            .iter()
            .find(|slot| !slot.id.is_base())
            .unwrap()
            .id
            .clone();
        runtime.activate_slot(&package, &previous).unwrap();
        let archive_id = ArchiveAccountId::new("account-capacity-slot").unwrap();
        let transfer_id = TransferId::new("transfer-capacity-slot").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                transfer_id.clone(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Base,
                    name: DisplayName::new("Base").unwrap(),
                    target: RestoreTarget::Existing {
                        slot: SlotId::base(),
                    },
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id.clone()),
                    launch_after_reboot: false,
                },
            )
            .unwrap();

        assert_eq!(
            runtime.commit_restore_account(&lease.io_token, &archive_id),
            Err(RuntimeError::InsufficientStorage),
        );
        let intent = runtime
            .packages
            .load_account_io_intent(&package)
            .unwrap()
            .unwrap();
        assert!(intent.maintenance_active);
        assert!(matches!(intent.phase, AccountIoPhase::Replacing { .. }));
        assert_eq!(runtime.android.view(&package), Some(&ObservedView::Base));
        runtime
            .slots
            .validate_staged_account(
                &package,
                &lease.io_token,
                &transfer_id,
                &archive_id,
                &RestorePolicy::Replace,
            )
            .unwrap();

        runtime.abort_account_io(&lease.io_token).unwrap();

        assert_eq!(runtime.get_package(&package).unwrap().active_slot, previous);
        assert_eq!(
            runtime.android.view(&package),
            Some(&ObservedView::Slot(previous)),
        );
    }

    #[test]
    fn base_capacity_check_leaves_maintenance_after_an_earlier_slot_restore() {
        let package = package();
        let mut android = MemoryAndroidOps::default();
        android.install(package.clone());
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            CapacityDeniedStorage::default(),
            android,
        );
        runtime.enroll(package.clone(), false, None).unwrap();
        let slot_archive = ArchiveAccountId::new("account-1").unwrap();
        let base_archive = ArchiveAccountId::new("base").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-slot-then-base").unwrap(),
                vec![
                    RestoreMapping {
                        archive_account_id: slot_archive.clone(),
                        archive_kind: ArchivedAccountKind::Slot,
                        name: DisplayName::new("Work").unwrap(),
                        target: RestoreTarget::New,
                        restore_policy: RestorePolicy::Replace,
                    },
                    RestoreMapping {
                        archive_account_id: base_archive.clone(),
                        archive_kind: ArchivedAccountKind::Base,
                        name: DisplayName::new("Base").unwrap(),
                        target: RestoreTarget::Existing {
                            slot: SlotId::base(),
                        },
                        restore_policy: RestorePolicy::Replace,
                    },
                ],
                ArchivedState {
                    scope: ArchiveScope::AllAccounts,
                    active_account_id: Some(base_archive.clone()),
                    launch_after_reboot: false,
                },
            )
            .unwrap();
        runtime
            .commit_restore_account(&lease.io_token, &slot_archive)
            .unwrap();
        assert!(matches!(
            runtime.android.view(&package),
            Some(ObservedView::Maintenance(_))
        ));

        assert_eq!(
            runtime.commit_restore_account(&lease.io_token, &base_archive),
            Err(RuntimeError::InsufficientStorage),
        );

        assert_eq!(runtime.android.view(&package), Some(&ObservedView::Base));
    }

    #[test]
    fn single_account_restore_replaces_base_without_launching_the_app() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        let archive_id = ArchiveAccountId::new("account-0").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-0").unwrap(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Base,
                    name: DisplayName::new("Ignored Base name").unwrap(),
                    target: RestoreTarget::Existing {
                        slot: SlotId::base(),
                    },
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id.clone()),
                    launch_after_reboot: true,
                },
            )
            .unwrap();

        let item = runtime
            .commit_restore_account(&lease.io_token, &archive_id)
            .unwrap();
        let result = runtime.finish_restore_io(&lease.io_token).unwrap();

        assert_eq!(item.state, RestoreItemState::Restored);
        assert_eq!(result.active_slot, SlotId::base());
        assert!(result.launch_after_reboot);
        assert!(!result.app_started);
        assert!(
            runtime
                .android
                .calls()
                .iter()
                .all(|call| !matches!(call, AndroidCall::Launch(..)))
        );
    }

    #[test]
    fn abort_after_final_cleanup_failure_finishes_the_committed_restore() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        let archive_id = ArchiveAccountId::new("account-1").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-cleanup-retry").unwrap(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Slot,
                    name: DisplayName::new("Restored work").unwrap(),
                    target: RestoreTarget::New,
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id.clone()),
                    launch_after_reboot: false,
                },
            )
            .unwrap();
        let restored = runtime
            .commit_restore_account(&lease.io_token, &archive_id)
            .unwrap();
        let target = restored.target_slot.clone().unwrap();
        runtime.slots.fail_next(SlotFailure::Cleanup);

        assert_eq!(
            runtime.finish_restore_io(&lease.io_token),
            Err(RuntimeError::StateConflict)
        );
        assert_eq!(runtime.get_package(&package).unwrap().active_slot, target);

        runtime.abort_account_io(&lease.io_token).unwrap();

        assert_eq!(runtime.get_package(&package).unwrap().active_slot, target);
        assert!(
            runtime
                .android
                .calls()
                .iter()
                .all(|call| !matches!(call, AndroidCall::Launch(..)))
        );
        assert!(
            runtime
                .packages
                .load_account_io_intent(&package)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn boot_recovery_after_final_cleanup_failure_preserves_the_committed_restore() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        let archive_id = ArchiveAccountId::new("account-1").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-cleanup-boot").unwrap(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Slot,
                    name: DisplayName::new("Restored work").unwrap(),
                    target: RestoreTarget::New,
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id.clone()),
                    launch_after_reboot: false,
                },
            )
            .unwrap();
        let restored = runtime
            .commit_restore_account(&lease.io_token, &archive_id)
            .unwrap();
        let target = restored.target_slot.clone().unwrap();
        runtime.slots.fail_next(SlotFailure::Cleanup);
        assert_eq!(
            runtime.finish_restore_io(&lease.io_token),
            Err(RuntimeError::StateConflict)
        );
        let (packages, slots, android) = runtime.into_parts();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        runtime.reconcile_boot().unwrap();

        assert_eq!(runtime.get_package(&package).unwrap().active_slot, target);
        assert!(
            runtime
                .packages
                .load_account_io_intent(&package)
                .unwrap()
                .is_none()
        );
        assert!(
            runtime.android.calls()[starting_calls..]
                .iter()
                .all(|call| !matches!(call, AndroidCall::Launch(..)))
        );
    }

    #[test]
    fn base_restore_leaves_the_maintenance_view_before_replacing_base() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        let archive_id = ArchiveAccountId::new("base").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-base-view").unwrap(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Base,
                    name: DisplayName::new("Base").unwrap(),
                    target: RestoreTarget::Existing {
                        slot: SlotId::base(),
                    },
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id.clone()),
                    launch_after_reboot: false,
                },
            )
            .unwrap();
        let starting_calls = runtime.android.calls().len();

        runtime
            .commit_restore_account(&lease.io_token, &archive_id)
            .unwrap();

        let calls = &runtime.android.calls()[starting_calls..];
        let maintenance = calls
            .iter()
            .position(
                |call| matches!(call, AndroidCall::Maintenance(value, _) if value == &package),
            )
            .unwrap();
        assert!(calls[maintenance + 1..].iter().any(|call| {
            matches!(call, AndroidCall::Apply(value, slot) if value == &package && slot.is_base())
        }));
        assert_eq!(runtime.android.view(&package), Some(&ObservedView::Base));
    }

    #[test]
    fn boot_recovery_leaves_maintenance_before_rolling_back_base() {
        let package = package();
        let (mut runtime, previous) = with_slot();
        runtime.activate_slot(&package, &previous).unwrap();
        let archive_id = ArchiveAccountId::new("base").unwrap();
        let transfer_id = TransferId::new("transfer-base-recovery").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                transfer_id.clone(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Base,
                    name: DisplayName::new("Base").unwrap(),
                    target: RestoreTarget::Existing {
                        slot: SlotId::base(),
                    },
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id.clone()),
                    launch_after_reboot: false,
                },
            )
            .unwrap();
        let mut intent = runtime
            .packages
            .load_account_io_intent(&package)
            .unwrap()
            .unwrap();
        intent.previous_enabled_state = Some(PackageEnabledState::Default);
        intent.maintenance_active = true;
        intent.phase = AccountIoPhase::Replacing {
            archive_account_id: archive_id.clone(),
            target_slot: SlotId::base(),
        };
        runtime.packages.save_account_io_intent(&intent).unwrap();
        runtime.android.block_launch(&package).unwrap();
        runtime
            .stop_and_apply_view(&package, &SlotId::base())
            .unwrap();
        runtime
            .slots
            .prepare_maintenance(&package, &lease.io_token)
            .unwrap();
        runtime
            .android
            .apply_maintenance_view(&package, &lease.io_token)
            .unwrap();
        runtime
            .slots
            .replace_account(
                &package,
                &lease.io_token,
                &transfer_id,
                &archive_id,
                &SlotId::base(),
                &RestorePolicy::Replace,
            )
            .unwrap();
        let (packages, slots, android) = runtime.into_parts();
        let starting_calls = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);

        runtime.reconcile_boot().unwrap();

        let snapshot = runtime.get_package(&package).unwrap();
        let calls = &runtime.android.calls()[starting_calls..];
        assert!(matches!(
            calls.iter().find(|call| matches!(call, AndroidCall::Apply(..))),
            Some(AndroidCall::Apply(value, slot)) if value == &package && slot.is_base()
        ));
        assert_eq!(snapshot.active_slot, previous);
        assert!(
            calls
                .iter()
                .all(|call| !matches!(call, AndroidCall::Launch(..)))
        );
    }

    #[test]
    fn restored_new_account_is_committed_to_the_ordinary_aggregate_only_after_data_success() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        let archive_id = ArchiveAccountId::new("account-1").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-1").unwrap(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Slot,
                    name: DisplayName::new("Restored work").unwrap(),
                    target: RestoreTarget::New,
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: Some(archive_id.clone()),
                    launch_after_reboot: false,
                },
            )
            .unwrap();

        let item = runtime
            .commit_restore_account(&lease.io_token, &archive_id)
            .unwrap();
        let result = runtime.finish_restore_io(&lease.io_token).unwrap();
        let snapshot = runtime.get_package(&package).unwrap();

        assert_eq!(item.state, RestoreItemState::Restored);
        assert_eq!(result.active_slot, item.target_slot.clone().unwrap());
        assert!(
            snapshot
                .slots
                .iter()
                .any(|slot| slot.id == result.active_slot && slot.name == "Restored work")
        );
    }

    #[test]
    fn abort_before_the_first_restore_commit_does_not_stop_or_relaunch_the_app() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        let archive_id = ArchiveAccountId::new("account-0").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-abort").unwrap(),
                vec![RestoreMapping {
                    archive_account_id: archive_id,
                    archive_kind: ArchivedAccountKind::Base,
                    name: DisplayName::new("Base").unwrap(),
                    target: RestoreTarget::Existing {
                        slot: SlotId::base(),
                    },
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: None,
                    launch_after_reboot: false,
                },
            )
            .unwrap();
        let starting_calls = runtime.android.calls().len();

        runtime.abort_account_io(&lease.io_token).unwrap();

        let (_packages, _slots, android) = runtime.into_parts();
        assert!(android.is_running(&package));
        assert!(
            !android.calls()[starting_calls..]
                .iter()
                .any(|call| matches!(call, AndroidCall::ForceStop(_) | AndroidCall::Launch(_, _)))
        );
    }

    #[test]
    fn abort_after_maintenance_never_relaunches_a_previously_disabled_package() {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), false, None).unwrap();
        let (packages, slots, mut android) = runtime.into_parts();
        android.set_enabled_state(&package, PackageEnabledState::DisabledUser);
        let mut runtime = Runtime::new(packages, slots, android);
        let archive_id = ArchiveAccountId::new("account-0").unwrap();
        let lease = runtime
            .begin_restore_io(
                &package,
                TransferId::new("transfer-disabled").unwrap(),
                vec![RestoreMapping {
                    archive_account_id: archive_id.clone(),
                    archive_kind: ArchivedAccountKind::Base,
                    name: DisplayName::new("Base").unwrap(),
                    target: RestoreTarget::Existing {
                        slot: SlotId::base(),
                    },
                    restore_policy: RestorePolicy::Replace,
                }],
                ArchivedState {
                    scope: ArchiveScope::Account,
                    active_account_id: None,
                    launch_after_reboot: false,
                },
            )
            .unwrap();
        runtime
            .commit_restore_account(&lease.io_token, &archive_id)
            .unwrap();
        let starting_calls = runtime.android.calls().len();

        runtime.abort_account_io(&lease.io_token).unwrap();

        let (_packages, _slots, android) = runtime.into_parts();
        assert!(
            !android.calls()[starting_calls..]
                .iter()
                .any(|call| matches!(call, AndroidCall::Launch(_, _)))
        );
    }

    #[test]
    fn single_account_restore_never_crosses_the_base_account_boundary() {
        let slot_archive_to_base = vec![RestoreMapping {
            archive_account_id: ArchiveAccountId::new("account-1").unwrap(),
            archive_kind: ArchivedAccountKind::Slot,
            name: DisplayName::new("Work").unwrap(),
            target: RestoreTarget::Existing {
                slot: SlotId::base(),
            },
            restore_policy: RestorePolicy::Replace,
        }];
        let base_archive_to_new = vec![RestoreMapping {
            archive_account_id: ArchiveAccountId::new("base").unwrap(),
            archive_kind: ArchivedAccountKind::Base,
            name: DisplayName::new("Base").unwrap(),
            target: RestoreTarget::New,
            restore_policy: RestorePolicy::Replace,
        }];
        let state = ArchivedState {
            scope: ArchiveScope::Account,
            active_account_id: None,
            launch_after_reboot: false,
        };

        assert_eq!(
            validate_restore_mapping_rules(&slot_archive_to_base, &state),
            Err(RuntimeError::InvalidRequest),
        );
        assert_eq!(
            validate_restore_mapping_rules(&base_archive_to_new, &state),
            Err(RuntimeError::InvalidRequest),
        );
    }
}
