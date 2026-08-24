use thiserror::Error;

use crate::model::{
    AccountIoIntent, AccountIoToken, ArchiveAccountId, Capabilities, ObservedView,
    PackageAggregate, PackageBinding, PackageEnabledState, PackageInspection, PackageName,
    RebindIntent, RestoreBatchResult, RestoreStaging, SeedMode, Slot, SlotId, TransferId,
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{message}")]
pub(crate) struct AdapterError {
    state_conflict: bool,
    insufficient_storage: bool,
    message: String,
}

impl AdapterError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            state_conflict: false,
            insufficient_storage: false,
            message: message.into(),
        }
    }

    pub(crate) fn state_conflict(message: impl Into<String>) -> Self {
        Self {
            state_conflict: true,
            insufficient_storage: false,
            message: message.into(),
        }
    }

    pub(crate) fn insufficient_storage(message: impl Into<String>) -> Self {
        Self {
            state_conflict: false,
            insufficient_storage: true,
            message: message.into(),
        }
    }

    pub(crate) fn is_state_conflict(&self) -> bool {
        self.state_conflict
    }

    pub(crate) fn is_insufficient_storage(&self) -> bool {
        self.insufficient_storage
    }
}

pub(crate) trait PackageStore: core::fmt::Debug {
    fn list(&self) -> Result<Vec<PackageName>, AdapterError>;
    fn load(&self, package: &PackageName) -> Result<Option<PackageAggregate>, AdapterError>;
    fn save(&mut self, aggregate: &PackageAggregate) -> Result<(), AdapterError>;
    fn load_binding(&self, package: &PackageName) -> Result<Option<PackageBinding>, AdapterError>;
    fn save_binding(
        &mut self,
        package: &PackageName,
        binding: &PackageBinding,
    ) -> Result<(), AdapterError>;
    fn load_rebind_intent(
        &self,
        package: &PackageName,
    ) -> Result<Option<RebindIntent>, AdapterError>;
    fn save_rebind_intent(&mut self, intent: &RebindIntent) -> Result<(), AdapterError>;
    fn clear_rebind_intent(&mut self, package: &PackageName) -> Result<(), AdapterError>;
    fn list_account_io_intents(&self) -> Result<Vec<AccountIoIntent>, AdapterError>;
    fn load_account_io_intent(
        &self,
        package: &PackageName,
    ) -> Result<Option<AccountIoIntent>, AdapterError>;
    fn save_account_io_intent(&mut self, intent: &AccountIoIntent) -> Result<(), AdapterError>;
    fn clear_account_io_intent(&mut self, package: &PackageName) -> Result<(), AdapterError>;
    fn save_account_io_result(
        &mut self,
        package: &PackageName,
        result: &RestoreBatchResult,
    ) -> Result<(), AdapterError>;
    fn backup_before_v1_binding(
        &mut self,
        aggregate: &PackageAggregate,
        complete_pairs: &[SlotId],
    ) -> Result<(), AdapterError>;
    fn remove(&mut self, package: &PackageName) -> Result<(), AdapterError>;
}

pub(crate) trait SlotStorage: core::fmt::Debug {
    fn materialize(
        &mut self,
        package: &PackageName,
        slot: &Slot,
        seed: SeedMode,
    ) -> Result<(), AdapterError>;

    fn discard(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError>;

    fn discard_package(&mut self, package: &PackageName) -> Result<(), AdapterError>;

    fn require_complete_pair(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<(), AdapterError>;

    fn account_paths(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<(String, String), AdapterError>;

    fn prepare_restore_staging(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        transfer_id: &TransferId,
        accounts: &[ArchiveAccountId],
    ) -> Result<Vec<RestoreStaging>, AdapterError>;

    fn validate_staged_account(
        &self,
        package: &PackageName,
        token: &AccountIoToken,
        transfer_id: &TransferId,
        account: &ArchiveAccountId,
    ) -> Result<(), AdapterError>;

    fn ensure_restore_capacity(
        &self,
        _package: &PackageName,
        _token: &AccountIoToken,
        _transfer_id: &TransferId,
        _account: &ArchiveAccountId,
        _target: &SlotId,
    ) -> Result<(), AdapterError> {
        Ok(())
    }

    fn prepare_maintenance(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(), AdapterError>;

    fn replace_account(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        transfer_id: &TransferId,
        account: &ArchiveAccountId,
        target: &SlotId,
    ) -> Result<(), AdapterError>;

    fn rollback_account(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        target: &SlotId,
    ) -> Result<(), AdapterError>;

    fn finalize_account(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        target: &SlotId,
    ) -> Result<(), AdapterError>;

    fn cleanup_account_io(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(), AdapterError>;
}

pub(crate) trait AndroidOps: core::fmt::Debug {
    fn probe(&mut self) -> Result<Capabilities, AdapterError>;
    fn inspect(&mut self, package: &PackageName) -> Result<PackageInspection, AdapterError>;
    fn force_stop(&mut self, package: &PackageName) -> Result<(), AdapterError>;
    fn observe_view(&mut self, package: &PackageName) -> Result<ObservedView, AdapterError>;
    fn apply_view(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError>;
    fn apply_maintenance_view(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(), AdapterError>;
    fn read_enabled_state(
        &mut self,
        package: &PackageName,
    ) -> Result<PackageEnabledState, AdapterError>;
    fn block_launch(&mut self, package: &PackageName) -> Result<(), AdapterError>;
    fn restore_enabled_state(
        &mut self,
        package: &PackageName,
        state: PackageEnabledState,
    ) -> Result<(), AdapterError>;
    fn launch_verified(
        &mut self,
        package: &PackageName,
        expected: &SlotId,
    ) -> Result<(), AdapterError>;
}
