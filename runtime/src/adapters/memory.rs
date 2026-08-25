use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::model::{
    AccountIoIntent, AccountIoToken, ArchiveAccountId, Capabilities, ObservedView,
    PackageAggregate, PackageBinding, PackageEnabledState, PackageIdentity, PackageInspection,
    PackageName, RebindIntent, RestoreBatchResult, RestoreStaging, SeedMode, Slot, SlotId,
    TransferId,
};
use crate::ports::{AdapterError, AndroidOps, PackageStore, SlotStorage};

#[derive(Debug, Default)]
pub(crate) struct MemoryPackageStore {
    packages: BTreeMap<PackageName, PackageAggregate>,
    bindings: BTreeMap<PackageName, PackageBinding>,
    rebind_intents: BTreeMap<PackageName, RebindIntent>,
    account_io_intents: BTreeMap<PackageName, AccountIoIntent>,
    account_io_results: BTreeMap<PackageName, RestoreBatchResult>,
    backups: BTreeMap<PackageName, (PackageAggregate, Vec<SlotId>)>,
    save_calls: usize,
    failed_save_calls: BTreeSet<usize>,
    remove_calls: usize,
    failed_remove_calls: BTreeSet<usize>,
}

impl MemoryPackageStore {
    pub(crate) fn fail_next_save(&mut self) {
        self.failed_save_calls.insert(self.save_calls + 1);
    }

    pub(crate) fn fail_save_call(&mut self, call: usize) {
        self.failed_save_calls.insert(call);
    }

    pub(crate) fn save_calls(&self) -> usize {
        self.save_calls
    }

    pub(crate) fn fail_next_remove(&mut self) {
        self.failed_remove_calls.insert(self.remove_calls + 1);
    }

    pub(crate) fn backup(&self, package: &PackageName) -> Option<&(PackageAggregate, Vec<SlotId>)> {
        self.backups.get(package)
    }
}

impl PackageStore for MemoryPackageStore {
    fn list(&self) -> Result<Vec<PackageName>, AdapterError> {
        Ok(self.packages.keys().cloned().collect())
    }

    fn load(&self, package: &PackageName) -> Result<Option<PackageAggregate>, AdapterError> {
        Ok(self.packages.get(package).cloned())
    }

    fn save(&mut self, aggregate: &PackageAggregate) -> Result<(), AdapterError> {
        self.save_calls += 1;
        if self.failed_save_calls.remove(&self.save_calls) {
            return Err(AdapterError::new("injected package-store failure"));
        }
        aggregate
            .validate()
            .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        self.packages
            .insert(aggregate.package().clone(), aggregate.clone());
        Ok(())
    }

    fn load_binding(&self, package: &PackageName) -> Result<Option<PackageBinding>, AdapterError> {
        Ok(self.bindings.get(package).cloned())
    }

    fn save_binding(
        &mut self,
        package: &PackageName,
        binding: &PackageBinding,
    ) -> Result<(), AdapterError> {
        binding
            .validate()
            .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        self.bindings.insert(package.clone(), binding.clone());
        Ok(())
    }

    fn load_rebind_intent(
        &self,
        package: &PackageName,
    ) -> Result<Option<RebindIntent>, AdapterError> {
        Ok(self.rebind_intents.get(package).cloned())
    }

    fn save_rebind_intent(&mut self, intent: &RebindIntent) -> Result<(), AdapterError> {
        intent
            .validate()
            .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        self.rebind_intents
            .insert(intent.package.clone(), intent.clone());
        Ok(())
    }

    fn clear_rebind_intent(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.rebind_intents.remove(package);
        Ok(())
    }

    fn list_account_io_intents(&self) -> Result<Vec<AccountIoIntent>, AdapterError> {
        Ok(self.account_io_intents.values().cloned().collect())
    }

    fn load_account_io_intent(
        &self,
        package: &PackageName,
    ) -> Result<Option<AccountIoIntent>, AdapterError> {
        Ok(self.account_io_intents.get(package).cloned())
    }

    fn save_account_io_intent(&mut self, intent: &AccountIoIntent) -> Result<(), AdapterError> {
        intent
            .validate()
            .map_err(|error| AdapterError::state_conflict(error.to_string()))?;
        self.account_io_intents
            .insert(intent.package.clone(), intent.clone());
        Ok(())
    }

    fn clear_account_io_intent(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.account_io_intents.remove(package);
        Ok(())
    }

    fn save_account_io_result(
        &mut self,
        package: &PackageName,
        result: &RestoreBatchResult,
    ) -> Result<(), AdapterError> {
        self.account_io_results
            .insert(package.clone(), result.clone());
        Ok(())
    }

    fn backup_before_v1_binding(
        &mut self,
        aggregate: &PackageAggregate,
        complete_pairs: &[SlotId],
    ) -> Result<(), AdapterError> {
        self.backups
            .entry(aggregate.package().clone())
            .or_insert_with(|| (aggregate.clone(), complete_pairs.to_vec()));
        Ok(())
    }

    fn remove(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.remove_calls += 1;
        if self.failed_remove_calls.remove(&self.remove_calls) {
            return Err(AdapterError::new("injected package-store remove failure"));
        }
        self.packages.remove(package);
        self.bindings.remove(package);
        self.rebind_intents.remove(package);
        self.account_io_intents.remove(package);
        self.account_io_results.remove(package);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlotFailure {
    MaterializeCe,
    MaterializeDe,
    Discard,
    DiscardCe,
    DiscardDe,
    DiscardPackageCe,
    DiscardPackageDe,
    Cleanup,
}

#[derive(Debug, Default)]
pub(crate) struct MemorySlotStorage {
    domains: BTreeMap<(PackageName, SlotId), (bool, bool)>,
    staging: BTreeSet<(PackageName, AccountIoToken, ArchiveAccountId)>,
    maintenance: BTreeSet<(PackageName, AccountIoToken)>,
    replacements: BTreeMap<(PackageName, AccountIoToken, SlotId), Option<(bool, bool)>>,
    failures: VecDeque<SlotFailure>,
}

impl MemorySlotStorage {
    pub(crate) fn contains(&self, package: &PackageName, slot: &SlotId) -> bool {
        self.domains
            .get(&(package.clone(), slot.clone()))
            .is_some_and(|pair| *pair == (true, true))
    }

    pub(crate) fn remove_de(&mut self, package: &PackageName, slot: &SlotId) {
        if let Some(pair) = self.domains.get_mut(&(package.clone(), slot.clone())) {
            pair.1 = false;
        }
    }

    pub(crate) fn domain_state(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Option<(bool, bool)> {
        self.domains.get(&(package.clone(), slot.clone())).copied()
    }

    pub(crate) fn fail_next(&mut self, failure: SlotFailure) {
        self.failures.push_back(failure);
    }

    fn take_failure(&mut self, expected: SlotFailure) -> bool {
        if self.failures.front() == Some(&expected) {
            self.failures.pop_front();
            true
        } else {
            false
        }
    }
}

impl SlotStorage for MemorySlotStorage {
    fn materialize(
        &mut self,
        package: &PackageName,
        slot: &Slot,
        _seed: SeedMode,
    ) -> Result<(), AdapterError> {
        let key = (package.clone(), slot.id().clone());
        if self.domains.contains_key(&key) {
            return Err(AdapterError::new("slot already materialized"));
        }
        if self.take_failure(SlotFailure::MaterializeCe) {
            return Err(AdapterError::new("injected CE materialization failure"));
        }
        self.domains.insert(key.clone(), (true, false));
        if self.take_failure(SlotFailure::MaterializeDe) {
            return Err(AdapterError::new("injected DE materialization failure"));
        }
        self.domains.insert(key, (true, true));
        Ok(())
    }

    fn discard(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError> {
        if self.take_failure(SlotFailure::Discard) {
            return Err(AdapterError::new("injected slot discard failure"));
        }
        let ce_failed = self.take_failure(SlotFailure::DiscardCe);
        let de_failed = self.take_failure(SlotFailure::DiscardDe);
        let key = (package.clone(), slot.clone());
        if let Some(pair) = self.domains.get_mut(&key) {
            if !ce_failed {
                pair.0 = false;
            }
            if !de_failed {
                pair.1 = false;
            }
        }
        self.domains.retain(|_, pair| *pair != (false, false));
        if ce_failed {
            Err(AdapterError::new("injected CE slot discard failure"))
        } else if de_failed {
            Err(AdapterError::new("injected DE slot discard failure"))
        } else {
            Ok(())
        }
    }

    fn discard_package(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        let ce_failed = self.take_failure(SlotFailure::DiscardPackageCe);
        let de_failed = self.take_failure(SlotFailure::DiscardPackageDe);
        for ((stored_package, _slot), pair) in &mut self.domains {
            if stored_package != package {
                continue;
            }
            if !ce_failed {
                pair.0 = false;
            }
            if !de_failed {
                pair.1 = false;
            }
        }
        self.domains.retain(|_, pair| *pair != (false, false));
        if ce_failed {
            Err(AdapterError::new("injected CE package discard failure"))
        } else if de_failed {
            Err(AdapterError::new("injected DE package discard failure"))
        } else {
            Ok(())
        }
    }

    fn require_complete_pair(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<(), AdapterError> {
        self.contains(package, slot)
            .then_some(())
            .ok_or_else(|| AdapterError::new("slot CE/DE pair is incomplete"))
    }

    fn account_paths(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<(String, String), AdapterError> {
        if !slot.is_base() {
            self.require_complete_pair(package, slot)?;
        }
        Ok((
            format!("/memory/ce/{package}/{slot}"),
            format!("/memory/de/{package}/{slot}"),
        ))
    }

    fn prepare_restore_staging(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        _transfer_id: &TransferId,
        accounts: &[ArchiveAccountId],
    ) -> Result<Vec<RestoreStaging>, AdapterError> {
        Ok(accounts
            .iter()
            .map(|account| {
                self.staging
                    .insert((package.clone(), token.clone(), account.clone()));
                RestoreStaging {
                    archive_account_id: account.clone(),
                    ce_path: format!(
                        "/memory/transfers/{}/{}/ce",
                        token.as_str(),
                        account.as_str()
                    ),
                    de_path: format!(
                        "/memory/transfers/{}/{}/de",
                        token.as_str(),
                        account.as_str()
                    ),
                }
            })
            .collect())
    }

    fn validate_staged_account(
        &self,
        package: &PackageName,
        token: &AccountIoToken,
        _transfer_id: &TransferId,
        account: &ArchiveAccountId,
    ) -> Result<(), AdapterError> {
        self.staging
            .contains(&(package.clone(), token.clone(), account.clone()))
            .then_some(())
            .ok_or_else(|| AdapterError::new("staged account is missing"))
    }

    fn prepare_maintenance(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(), AdapterError> {
        self.maintenance.insert((package.clone(), token.clone()));
        Ok(())
    }

    fn replace_account(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        transfer_id: &TransferId,
        account: &ArchiveAccountId,
        target: &SlotId,
    ) -> Result<(), AdapterError> {
        self.validate_staged_account(package, token, transfer_id, account)?;
        let key = (package.clone(), token.clone(), target.clone());
        self.replacements.entry(key).or_insert_with(|| {
            if target.is_base() {
                Some((true, true))
            } else {
                self.domains
                    .get(&(package.clone(), target.clone()))
                    .copied()
            }
        });
        if !target.is_base() {
            self.domains
                .insert((package.clone(), target.clone()), (true, true));
        }
        Ok(())
    }

    fn rollback_account(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        target: &SlotId,
    ) -> Result<(), AdapterError> {
        let key = (package.clone(), token.clone(), target.clone());
        if let Some(previous) = self.replacements.remove(&key)
            && !target.is_base()
        {
            match previous {
                Some(pair) => {
                    self.domains.insert((package.clone(), target.clone()), pair);
                }
                None => {
                    self.domains.remove(&(package.clone(), target.clone()));
                }
            }
        }
        Ok(())
    }

    fn finalize_account(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
        target: &SlotId,
    ) -> Result<(), AdapterError> {
        self.replacements
            .remove(&(package.clone(), token.clone(), target.clone()));
        Ok(())
    }

    fn cleanup_account_io(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(), AdapterError> {
        if self.take_failure(SlotFailure::Cleanup) {
            return Err(AdapterError::new("injected account-I/O cleanup failure"));
        }
        self.staging.retain(|(stored_package, stored_token, _)| {
            stored_package != package || stored_token != token
        });
        self.maintenance.remove(&(package.clone(), token.clone()));
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AndroidFailure {
    Inspect,
    ForceStop,
    Observe,
    Apply,
    Launch,
    Maintenance,
    BlockLaunch,
    RestoreEnabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AndroidCall {
    Inspect(PackageName),
    ForceStop(PackageName),
    Observe(PackageName),
    Apply(PackageName, SlotId),
    Launch(PackageName, SlotId),
    Maintenance(PackageName, AccountIoToken),
    BlockLaunch(PackageName),
    RestoreEnabled(PackageName, PackageEnabledState),
}

#[derive(Debug)]
pub(crate) struct MemoryAndroidOps {
    build_id: String,
    inspections: BTreeMap<PackageName, PackageInspection>,
    views: BTreeMap<PackageName, ObservedView>,
    running: BTreeSet<PackageName>,
    enabled: BTreeMap<PackageName, PackageEnabledState>,
    failures: VecDeque<AndroidFailure>,
    calls: Vec<AndroidCall>,
}

impl Default for MemoryAndroidOps {
    fn default() -> Self {
        Self {
            build_id: "memory".to_owned(),
            inspections: BTreeMap::new(),
            views: BTreeMap::new(),
            running: BTreeSet::new(),
            enabled: BTreeMap::new(),
            failures: VecDeque::new(),
            calls: Vec::new(),
        }
    }
}

impl MemoryAndroidOps {
    pub(crate) fn install(&mut self, package: PackageName) {
        let Ok(identity) =
            PackageIdentity::new(10_000, format!("/data/app/{package}/base.apk"), 1, 2)
        else {
            return;
        };
        self.inspections.insert(
            package.clone(),
            PackageInspection {
                identity,
                was_running: true,
            },
        );
        self.views.insert(package.clone(), ObservedView::Base);
        self.running.insert(package.clone());
        self.enabled.insert(package, PackageEnabledState::Default);
    }

    pub(crate) fn fail_next(&mut self, failure: AndroidFailure) {
        self.failures.push_back(failure);
    }

    pub(crate) fn set_view(&mut self, package: &PackageName, view: ObservedView) {
        self.views.insert(package.clone(), view);
    }

    pub(crate) fn change_identity(&mut self, package: &PackageName) {
        self.change_identity_to(package, 99);
    }

    pub(crate) fn change_identity_to(&mut self, package: &PackageName, inode: u64) {
        if let Some(inspection) = self.inspections.get_mut(package) {
            let Ok(identity) =
                PackageIdentity::new(10_000, format!("/data/app/{package}/base.apk"), 1, inode)
            else {
                return;
            };
            inspection.identity = identity;
        }
    }

    pub(crate) fn change_uid(&mut self, package: &PackageName) {
        if let Some(inspection) = self.inspections.get_mut(package) {
            let Ok(identity) =
                PackageIdentity::new(20_000, format!("/data/app/{package}/base.apk"), 1, 99)
            else {
                return;
            };
            inspection.identity = identity;
        }
    }

    pub(crate) fn calls(&self) -> &[AndroidCall] {
        &self.calls
    }

    pub(crate) fn view(&self, package: &PackageName) -> Option<&ObservedView> {
        self.views.get(package)
    }

    pub(crate) fn is_running(&self, package: &PackageName) -> bool {
        self.running.contains(package)
    }

    pub(crate) fn set_enabled_state(&mut self, package: &PackageName, state: PackageEnabledState) {
        self.enabled.insert(package.clone(), state);
    }

    pub(crate) fn enabled_state(&self, package: &PackageName) -> Option<PackageEnabledState> {
        self.enabled.get(package).copied()
    }

    fn take_failure(&mut self, expected: AndroidFailure) -> bool {
        if self.failures.front() == Some(&expected) {
            self.failures.pop_front();
            true
        } else {
            false
        }
    }

    fn require_installed(&self, package: &PackageName) -> Result<(), AdapterError> {
        self.inspections
            .contains_key(package)
            .then_some(())
            .ok_or_else(|| AdapterError::new("package is not installed"))
    }
}

impl AndroidOps for MemoryAndroidOps {
    fn probe(&mut self) -> Result<Capabilities, AdapterError> {
        Ok(Capabilities {
            build_id: self.build_id.clone(),
        })
    }

    fn inspect(&mut self, package: &PackageName) -> Result<PackageInspection, AdapterError> {
        self.calls.push(AndroidCall::Inspect(package.clone()));
        if self.take_failure(AndroidFailure::Inspect) {
            return Err(AdapterError::new("injected package inspection failure"));
        }
        let mut inspection = self
            .inspections
            .get(package)
            .cloned()
            .ok_or_else(|| AdapterError::new("package is not installed"))?;
        inspection.was_running = self.running.contains(package);
        Ok(inspection)
    }

    fn force_stop(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.calls.push(AndroidCall::ForceStop(package.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::ForceStop) {
            self.running.remove(package);
            return Err(AdapterError::new("injected force-stop failure"));
        }
        self.running.remove(package);
        Ok(())
    }

    fn observe_view(&mut self, package: &PackageName) -> Result<ObservedView, AdapterError> {
        self.calls.push(AndroidCall::Observe(package.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::Observe) {
            return Err(AdapterError::new("injected view observation failure"));
        }
        self.views
            .get(package)
            .cloned()
            .ok_or_else(|| AdapterError::new("package view is unavailable"))
    }

    fn apply_view(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError> {
        self.calls
            .push(AndroidCall::Apply(package.clone(), slot.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::Apply) {
            return Err(AdapterError::new("injected view application failure"));
        }
        self.views
            .insert(package.clone(), ObservedView::for_slot(slot));
        Ok(())
    }

    fn apply_maintenance_view(
        &mut self,
        package: &PackageName,
        token: &AccountIoToken,
    ) -> Result<(), AdapterError> {
        self.calls
            .push(AndroidCall::Maintenance(package.clone(), token.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::Maintenance) {
            return Err(AdapterError::new("injected maintenance-view failure"));
        }
        self.views
            .insert(package.clone(), ObservedView::Maintenance(token.clone()));
        Ok(())
    }

    fn read_enabled_state(
        &mut self,
        package: &PackageName,
    ) -> Result<PackageEnabledState, AdapterError> {
        self.require_installed(package)?;
        Ok(self
            .enabled
            .get(package)
            .copied()
            .unwrap_or(PackageEnabledState::Default))
    }

    fn block_launch(&mut self, package: &PackageName) -> Result<(), AdapterError> {
        self.calls.push(AndroidCall::BlockLaunch(package.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::BlockLaunch) {
            self.enabled
                .insert(package.clone(), PackageEnabledState::DisabledUser);
            self.running.remove(package);
            return Err(AdapterError::new("injected launch-block failure"));
        }
        self.enabled
            .insert(package.clone(), PackageEnabledState::DisabledUser);
        self.running.remove(package);
        Ok(())
    }

    fn restore_enabled_state(
        &mut self,
        package: &PackageName,
        state: PackageEnabledState,
    ) -> Result<(), AdapterError> {
        self.calls
            .push(AndroidCall::RestoreEnabled(package.clone(), state));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::RestoreEnabled) {
            return Err(AdapterError::new("injected enabled-state failure"));
        }
        self.enabled.insert(package.clone(), state);
        Ok(())
    }

    fn launch_verified(
        &mut self,
        package: &PackageName,
        expected: &SlotId,
    ) -> Result<(), AdapterError> {
        self.calls
            .push(AndroidCall::Launch(package.clone(), expected.clone()));
        self.require_installed(package)?;
        if self.take_failure(AndroidFailure::Launch) {
            return Err(AdapterError::new("injected launch failure"));
        }
        self.views
            .get(package)
            .is_some_and(|view| view.matches(expected))
            .then_some(())
            .ok_or_else(|| AdapterError::new("launched process has the wrong view"))?;
        self.running.insert(package.clone());
        Ok(())
    }
}
