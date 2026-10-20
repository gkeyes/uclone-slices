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

    fn stage_package_trash(&mut self, package: &PackageName) -> Result<(bool, bool), AdapterError> {
        self.0.stage_package_trash(package)
    }

    fn reserve_trash_name(&self, package: &PackageName) -> Result<String, AdapterError> {
        self.0.reserve_trash_name(package)
    }

    fn finalize_package_trash(
        &mut self,
        intent: &crate::model::TrashIntent,
    ) -> Result<(), AdapterError> {
        self.0.finalize_package_trash(intent)
    }

    fn purge_trash(&mut self, held: &[crate::model::TrashHold]) -> Result<(), AdapterError> {
        self.0.purge_trash(held)
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
    runtime.enroll(package.clone(), None).unwrap();
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
    runtime.enroll(failed.clone(), None).unwrap();
    runtime.enroll(healthy.clone(), None).unwrap();
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
    let first = runtime.enroll(package.clone(), None).unwrap();

    let second = runtime.enroll(package, None).unwrap();

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
    runtime.enroll(package.clone(), Some(signing())).unwrap();
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
    runtime.enroll(changed.clone(), Some(signing())).unwrap();
    runtime.enroll(untouched.clone(), Some(signing())).unwrap();
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
    runtime.enroll(package.clone(), Some(signing())).unwrap();
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
    runtime.enroll(package.clone(), Some(signing())).unwrap();
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
    runtime.enroll(package.clone(), Some(signing())).unwrap();
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
    runtime.enroll(package.clone(), Some(signing())).unwrap();
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
    runtime.enroll(package.clone(), Some(signing())).unwrap();
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
fn save_intent_failure_does_not_touch_slot_storage() {
    let package = package();
    let mut runtime = runtime();
    runtime.enroll(package.clone(), None).unwrap();
    let (mut packages, slots, android) = runtime.into_parts();
    packages.fail_next_save();
    let mut runtime = Runtime::new(packages, slots, android);

    let result = runtime.create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank);

    assert_eq!(result, Err(RuntimeError::OperationFailed));
    let (_packages, slots, _android) = runtime.into_parts();
    assert!(!slots.contains(&package, &SlotId::numbered(1)));
}

#[test]
fn insufficient_clone_capacity_is_reported_before_any_side_effect() {
    let package = package();
    let mut runtime = runtime();
    runtime.enroll(package.clone(), None).unwrap();
    let (packages, mut slots, android) = runtime.into_parts();
    slots.fail_next(SlotFailure::MaterializeCapacity);
    let saves_before = packages.save_calls();
    let mut runtime = Runtime::new(packages, slots, android);
    let calls_before = runtime.android.calls().len();

    let result = runtime.create_slot(
        &package,
        DisplayName::new("Copy").unwrap(),
        SeedMode::CloneBase,
    );

    assert_eq!(result, Err(RuntimeError::InsufficientStorage));
    let (packages, slots, android) = runtime.into_parts();
    assert_eq!(packages.save_calls(), saves_before);
    assert!(!slots.contains(&package, &SlotId::numbered(1)));
    assert!(
        !android.calls()[calls_before..]
            .iter()
            .any(|call| matches!(call, AndroidCall::ForceStop(_)))
    );
    assert!(android.is_running(&package));
}

#[test]
fn one_unreadable_account_io_intent_does_not_block_other_packages_or_boot() {
    use crate::adapters::FilePackageStore;

    let root = tempfile::tempdir().unwrap();
    let healthy = package();
    let mut android = MemoryAndroidOps::default();
    android.install(healthy.clone());
    let mut runtime = Runtime::new(
        FilePackageStore::open(root.path()).unwrap(),
        MemorySlotStorage::default(),
        android,
    );
    runtime.enroll(healthy.clone(), None).unwrap();
    let broken = PackageName::new("com.other.app").unwrap();
    let broken_root = root.path().join("packages").join(broken.as_str());
    std::fs::create_dir_all(&broken_root).unwrap();
    std::fs::write(
        broken_root.join("account-io-intent-v1.json"),
        br#"{"schema_version":99}"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.path().join("packages").join("Not A Package")).unwrap();

    assert_eq!(runtime.reconcile_boot(), Ok(()));
    assert_eq!(runtime.list_account_io_status(), Ok(Vec::new()));
    let lease = runtime
        .begin_backup_io(&healthy, AccountIoScope::AllAccounts)
        .unwrap();
    assert_eq!(runtime.list_account_io_status().unwrap().len(), 1);
    assert_eq!(runtime.finish_backup_io(&lease.io_token), Ok(()));
    assert_eq!(runtime.list_packages().unwrap().len(), 1);
    // The unreadable package itself stays fail-closed.
    assert!(runtime.unenroll(&broken).is_err());
    assert!(broken_root.join("account-io-intent-v1.json").is_file());
}

#[test]
fn each_domain_materialization_failure_aborts_creation() {
    for failure in [SlotFailure::MaterializeCe, SlotFailure::MaterializeDe] {
        let package = package();
        let mut runtime = runtime();
        runtime.enroll(package.clone(), None).unwrap();
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
    runtime.enroll(package.clone(), None).unwrap();
    let (packages, mut slots, android) = runtime.into_parts();
    slots.fail_next(SlotFailure::Discard);
    let mut runtime = Runtime::new(packages, slots, android);

    let result = runtime.create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank);

    assert_eq!(result, Err(RuntimeError::OperationFailed));
    let (packages, slots, _android) = runtime.into_parts();
    assert!(!slots.contains(&package, &SlotId::numbered(1)));
    assert!(packages.load(&package).unwrap().unwrap().is_ready());
}

#[test]
fn interrupted_creation_discards_pair_and_restores_running_app() {
    let package = package();
    let mut runtime = runtime();
    runtime.enroll(package.clone(), None).unwrap();
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
    runtime.enroll(package.clone(), None).unwrap();
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
    runtime.enroll(package.clone(), None).unwrap();
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
        runtime.enroll(package.clone(), None).unwrap();
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
        runtime.enroll(package.clone(), None).unwrap();
        let target = runtime
            .create_slot(package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
            .unwrap()
            .slots[1]
            .id
            .clone();
        runtime.activate_slot(package, &target).unwrap();
        targets.push((package.clone(), target));
    }
    runtime.enroll(base.clone(), None).unwrap();
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
        runtime.enroll(package.clone(), None).unwrap();
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
fn unenroll_from_active_slot_returns_to_base_and_trashes_every_slot() {
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
    assert_eq!(slots.trashed(&package, &target), Some((true, true)));
    assert_eq!(slots.purge_calls(), 1);
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
    runtime.enroll(package.clone(), None).unwrap();

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
        (SlotFailure::TrashPackageCe, (true, false)),
        (SlotFailure::TrashPackageDe, (false, true)),
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
        assert_eq!(
            slots.pending_trash(&package, &target),
            Some((!remaining.0, !remaining.1))
        );
        assert_eq!(slots.trashed(&package, &target), None);
        assert!(packages.load(&package).unwrap().unwrap().is_unenrolling());

        let mut runtime = Runtime::new(packages, slots, android);
        assert_eq!(runtime.get_package(&package), Err(RuntimeError::NotFound));
        let (packages, slots, _android) = runtime.into_parts();
        assert!(packages.load(&package).unwrap().is_none());
        assert_eq!(slots.domain_state(&package, &target), None);
        assert_eq!(slots.pending_trash(&package, &target), None);
        assert_eq!(slots.trashed(&package, &target), Some((true, true)));
    }
}

#[test]
fn unenroll_purge_failure_does_not_fail_the_unenroll() {
    let package = package();
    let (runtime, target) = with_slot();
    let (packages, mut slots, android) = runtime.into_parts();
    slots.fail_next(SlotFailure::PurgeTrash);
    let mut runtime = Runtime::new(packages, slots, android);

    assert_eq!(runtime.unenroll(&package), Ok(()));

    let (packages, slots, _android) = runtime.into_parts();
    assert!(packages.load(&package).unwrap().is_none());
    assert_eq!(slots.trashed(&package, &target), Some((true, true)));
    assert_eq!(slots.purge_calls(), 1);
}

#[test]
fn failed_unenroll_does_not_purge_and_boot_reconcile_purges() {
    let package = package();
    let (runtime, _target) = with_slot();
    let (packages, mut slots, android) = runtime.into_parts();
    slots.fail_next(SlotFailure::TrashPackageCe);
    let mut runtime = Runtime::new(packages, slots, android);
    assert_eq!(
        runtime.unenroll(&package),
        Err(RuntimeError::OperationFailed)
    );
    assert_eq!(runtime.slots.purge_calls(), 0);

    runtime.reconcile_boot().unwrap();

    let (packages, slots, _android) = runtime.into_parts();
    assert!(packages.load(&package).unwrap().is_none());
    assert_eq!(slots.purge_calls(), 1);
}

#[test]
fn interrupted_trash_finalize_recovers_under_the_same_trash_name() {
    let package = package();
    let (runtime, target) = with_slot();
    let (packages, mut slots, android) = runtime.into_parts();
    slots.fail_next(SlotFailure::FinalizeTrashDe);
    let mut runtime = Runtime::new(packages, slots, android);

    assert_eq!(
        runtime.unenroll(&package),
        Err(RuntimeError::OperationFailed)
    );
    let (packages, slots, android) = runtime.into_parts();
    assert!(packages.load(&package).unwrap().unwrap().is_unenrolling());
    assert_eq!(
        packages.load_trash_intent(&package).unwrap().unwrap().name,
        "1000"
    );
    assert_eq!(
        slots.trash_names(&package, &target),
        Some((Some("1000".to_owned()), None))
    );
    assert_eq!(slots.pending_trash(&package, &target), Some((false, true)));

    let mut runtime = Runtime::new(packages, slots, android);
    assert_eq!(runtime.get_package(&package), Err(RuntimeError::NotFound));
    let (packages, slots, _android) = runtime.into_parts();
    assert!(packages.load(&package).unwrap().is_none());
    assert!(packages.load_trash_intent(&package).unwrap().is_none());
    assert_eq!(
        slots.trash_names(&package, &target),
        Some((Some("1000".to_owned()), Some("1000".to_owned())))
    );
}

#[test]
fn purge_holds_only_the_trash_entries_of_unenrolls_in_progress() {
    let package = package();
    let stuck = PackageName::new("com.example.stuck").unwrap();
    let staged = PackageName::new("com.example.staged").unwrap();
    let unreadable = PackageName::new("com.example.unreadable").unwrap();
    let mut runtime = runtime();
    for other in [&stuck, &staged, &unreadable] {
        runtime.android.install(other.clone());
    }
    runtime.enroll(package.clone(), None).unwrap();
    for other in [&stuck, &staged, &unreadable] {
        runtime.enroll(other.clone(), None).unwrap();
    }
    let (mut packages, slots, android) = runtime.into_parts();
    // `staged` is unenrolling but has no trash intent yet, so its data is still pending and
    // none of its older trash needs to be held.
    for other in [&stuck, &staged] {
        let mut aggregate = packages.load(other).unwrap().unwrap();
        aggregate.begin_unenrollment().unwrap();
        packages.save(&aggregate).unwrap();
    }
    packages
        .save_trash_intent(&TrashIntent {
            package: stuck.clone(),
            name: "1000".to_owned(),
            ce: true,
            de: true,
        })
        .unwrap();
    packages.make_trash_intent_unreadable(unreadable.clone());
    let mut runtime = Runtime::new(packages, slots, android);

    runtime.unenroll(&package).unwrap();

    assert_eq!(
        runtime.slots.last_purge_held(),
        &[
            TrashHold::Entry {
                package: stuck,
                name: "1000".to_owned(),
            },
            TrashHold::Package(unreadable),
        ]
    );
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
        Err(RuntimeError::StateConflict),
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
fn failed_boot_recovery_is_reported_and_retried_on_the_next_reconcile() {
    let package = package();
    let (mut runtime, active) = with_slot();
    runtime.activate_slot(&package, &active).unwrap();
    runtime
        .begin_backup_io(&package, AccountIoScope::AllAccounts)
        .unwrap();
    let (packages, slots, mut android) = runtime.into_parts();
    android.fail_next(AndroidFailure::RestoreEnabled);
    let mut runtime = Runtime::new(packages, slots, android);

    assert_eq!(runtime.reconcile_boot(), Err(RuntimeError::StateConflict));
    assert!(
        runtime
            .packages
            .load_account_io_intent(&package)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        runtime.android.enabled_state(&package),
        Some(PackageEnabledState::DisabledUser),
    );

    assert_eq!(runtime.reconcile_boot(), Ok(()));
    assert!(
        runtime
            .packages
            .load_account_io_intent(&package)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        runtime.android.enabled_state(&package),
        Some(PackageEnabledState::Default),
    );
}

#[test]
fn restore_lease_setup_failure_keeps_the_intent_when_staging_cleanup_fails() {
    let package = package();
    let mut runtime = runtime();
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-1").unwrap();
    runtime.packages.fail_next_save();
    runtime.slots.fail_next(SlotFailure::Cleanup);

    let result = runtime.begin_restore_io(
        &package,
        TransferId::new("transfer-setup-cleanup").unwrap(),
        vec![RestoreMapping {
            archive_account_id: archive_id,
            archive_kind: ArchiveAccountKind::Slot,
            name: DisplayName::new("Restored work").unwrap(),
            target: RestoreTarget::New,
            restore_policy: RestorePolicy::Replace,
        }],
        ArchivedState {
            scope: ArchiveScope::Account,
            active_account_id: None,
            launch_after_reboot: false,
        },
    );

    assert_eq!(result.err(), Some(RuntimeError::StateConflict));
    assert!(
        runtime
            .packages
            .load_account_io_intent(&package)
            .unwrap()
            .is_some()
    );

    assert_eq!(runtime.reconcile_boot(), Ok(()));
    assert!(
        runtime
            .packages
            .load_account_io_intent(&package)
            .unwrap()
            .is_none()
    );
    assert_eq!(runtime.get_package(&package).unwrap().slots.len(), 1);
}

fn begin_new_slot_restore(runtime: &mut TestRuntime) -> (AccountIoToken, ArchiveAccountId) {
    let package = package();
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-1").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-metadata").unwrap(),
            vec![RestoreMapping {
                archive_account_id: archive_id.clone(),
                archive_kind: ArchiveAccountKind::Slot,
                name: DisplayName::new("Restored work").unwrap(),
                target: RestoreTarget::New,
                restore_policy: RestorePolicy::Replace,
            }],
            ArchivedState {
                scope: ArchiveScope::Account,
                active_account_id: None,
                launch_after_reboot: false,
            },
        )
        .unwrap();
    (lease.io_token, archive_id)
}

#[test]
fn restore_metadata_save_failure_rolls_back_and_records_the_item_as_failed() {
    let package = package();
    let mut runtime = runtime();
    let (token, archive_id) = begin_new_slot_restore(&mut runtime);
    runtime.packages.fail_next_save();

    let item = runtime.commit_restore_account(&token, &archive_id).unwrap();

    assert_eq!(item.state, RestoreItemState::Failed);
    let target = item.target_slot.unwrap();
    assert!(!runtime.slots.contains(&package, &target));
    let intent = runtime
        .packages
        .load_account_io_intent(&package)
        .unwrap()
        .unwrap();
    assert_eq!(intent.phase, AccountIoPhase::Ready);
}

#[test]
fn unrecoverable_restore_metadata_keeps_the_replacing_intent_for_recovery() {
    let package = package();
    let mut runtime = runtime();
    let (token, archive_id) = begin_new_slot_restore(&mut runtime);
    let next_save = runtime.packages.save_calls() + 1;
    runtime.packages.fail_save_call(next_save);
    runtime.packages.fail_save_call(next_save + 1);

    assert_eq!(
        runtime.commit_restore_account(&token, &archive_id),
        Err(RuntimeError::StateConflict)
    );
    let intent = runtime
        .packages
        .load_account_io_intent(&package)
        .unwrap()
        .unwrap();
    let target_slot = match intent.phase {
        AccountIoPhase::Replacing { target_slot, .. } => Some(target_slot),
        _ => None,
    }
    .unwrap();
    assert!(intent.completed.is_empty());
    assert!(runtime.slots.contains(&package, &target_slot));

    runtime.abort_account_io(&token).unwrap();

    assert!(!runtime.slots.contains(&package, &target_slot));
    assert_eq!(runtime.get_package(&package).unwrap().slots.len(), 1);
    assert!(
        runtime
            .packages
            .load_account_io_intent(&package)
            .unwrap()
            .is_none()
    );
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
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-0").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-order").unwrap(),
            vec![RestoreMapping {
                archive_account_id: archive_id.clone(),
                archive_kind: ArchiveAccountKind::Base,
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
    assert!(matches!(calls.first(), Some(AndroidCall::BlockLaunch(value)) if value == &package));
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
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-policy").unwrap();
    let policy = RestorePolicy::PreservePaths {
        paths: vec![RestorePath {
            domain: ArchiveDomain::Ce,
            path: "files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer".to_owned(),
        }],
    };

    runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-policy").unwrap(),
            vec![RestoreMapping {
                archive_account_id: archive_id.clone(),
                archive_kind: ArchiveAccountKind::Base,
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
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-capacity").unwrap();
    let transfer_id = TransferId::new("transfer-capacity").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            transfer_id.clone(),
            vec![RestoreMapping {
                archive_account_id: archive_id.clone(),
                archive_kind: ArchiveAccountKind::Base,
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
    runtime.enroll(package.clone(), None).unwrap();
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
                archive_kind: ArchiveAccountKind::Base,
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
    runtime.enroll(package.clone(), None).unwrap();
    let slot_archive = ArchiveAccountId::new("account-1").unwrap();
    let base_archive = ArchiveAccountId::new("base").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-slot-then-base").unwrap(),
            vec![
                RestoreMapping {
                    archive_account_id: slot_archive.clone(),
                    archive_kind: ArchiveAccountKind::Slot,
                    name: DisplayName::new("Work").unwrap(),
                    target: RestoreTarget::New,
                    restore_policy: RestorePolicy::Replace,
                },
                RestoreMapping {
                    archive_account_id: base_archive.clone(),
                    archive_kind: ArchiveAccountKind::Base,
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
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-0").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-0").unwrap(),
            vec![RestoreMapping {
                archive_account_id: archive_id.clone(),
                archive_kind: ArchiveAccountKind::Base,
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
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-1").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-cleanup-retry").unwrap(),
            vec![RestoreMapping {
                archive_account_id: archive_id.clone(),
                archive_kind: ArchiveAccountKind::Slot,
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
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-1").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-cleanup-boot").unwrap(),
            vec![RestoreMapping {
                archive_account_id: archive_id.clone(),
                archive_kind: ArchiveAccountKind::Slot,
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
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("base").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-base-view").unwrap(),
            vec![RestoreMapping {
                archive_account_id: archive_id.clone(),
                archive_kind: ArchiveAccountKind::Base,
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
        .position(|call| matches!(call, AndroidCall::Maintenance(value, _) if value == &package))
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
                archive_kind: ArchiveAccountKind::Base,
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
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-1").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-1").unwrap(),
            vec![RestoreMapping {
                archive_account_id: archive_id.clone(),
                archive_kind: ArchiveAccountKind::Slot,
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
    runtime.enroll(package.clone(), None).unwrap();
    let archive_id = ArchiveAccountId::new("account-0").unwrap();
    let lease = runtime
        .begin_restore_io(
            &package,
            TransferId::new("transfer-abort").unwrap(),
            vec![RestoreMapping {
                archive_account_id: archive_id,
                archive_kind: ArchiveAccountKind::Base,
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
    runtime.enroll(package.clone(), None).unwrap();
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
                archive_kind: ArchiveAccountKind::Base,
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
        archive_kind: ArchiveAccountKind::Slot,
        name: DisplayName::new("Work").unwrap(),
        target: RestoreTarget::Existing {
            slot: SlotId::base(),
        },
        restore_policy: RestorePolicy::Replace,
    }];
    let base_archive_to_new = vec![RestoreMapping {
        archive_account_id: ArchiveAccountId::new("base").unwrap(),
        archive_kind: ArchiveAccountKind::Base,
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

#[test]
fn default_account_return_uses_the_normal_transaction_and_is_idempotent() {
    let package = package();
    let (mut runtime, slot) = with_slot();
    runtime
        .set_default_slot(&package, Some(SlotId::base()))
        .unwrap();
    runtime.activate_slot(&package, &slot).unwrap();
    assert!(runtime.activate_default_account(&package, || true).unwrap());
    let snapshot = runtime.get_package(&package).unwrap();
    assert_eq!(snapshot.active_slot, SlotId::base());
    assert_eq!(snapshot.default_slot, Some(SlotId::base()));
    let (packages, slots, android) = runtime.into_parts();
    let calls_before = android.calls().len();
    let mut runtime = Runtime::new(packages, slots, android);
    assert!(!runtime.activate_default_account(&package, || true).unwrap());
    let (_packages, _slots, android) = runtime.into_parts();
    assert_eq!(android.calls().len(), calls_before);
}

#[test]
fn default_account_return_cancels_if_the_user_leaves_during_preflight_or_journaling() {
    for permitted_checks in [0, 1] {
        let package = package();
        let (mut runtime, slot) = with_slot();
        runtime
            .set_default_slot(&package, Some(SlotId::base()))
            .unwrap();
        runtime.activate_slot(&package, &slot).unwrap();
        let (packages, slots, android) = runtime.into_parts();
        let calls_before = android.calls().len();
        let mut runtime = Runtime::new(packages, slots, android);
        let mut checks = 0;

        assert!(
            !runtime
                .activate_default_account(&package, || {
                    checks += 1;
                    checks <= permitted_checks
                })
                .unwrap()
        );

        let (packages, _slots, android) = runtime.into_parts();
        let aggregate = packages.load(&package).unwrap().unwrap();
        assert!(aggregate.is_ready());
        assert_eq!(aggregate.active_slot(), &slot);
        assert_eq!(aggregate.default_slot(), Some(&SlotId::base()));
        assert!(android.view(&package).unwrap().matches(&slot));
        assert_eq!(checks, permitted_checks + 1);
        assert!(!android.calls()[calls_before..].iter().any(|call| matches!(
            call,
            AndroidCall::ForceStop(_) | AndroidCall::Apply(_, _) | AndroidCall::Launch(_, _)
        )));
    }
}

#[test]
fn automatic_rollback_restores_the_account_without_reopening_after_departure() {
    let package = package();
    let (mut runtime, slot) = with_slot();
    runtime
        .set_default_slot(&package, Some(SlotId::base()))
        .unwrap();
    runtime.activate_slot(&package, &slot).unwrap();
    let (packages, slots, mut android) = runtime.into_parts();
    let calls_before = android.calls().len();
    android.fail_next(AndroidFailure::Apply);
    let mut runtime = Runtime::new(packages, slots, android);
    let mut checks = 0;

    assert_eq!(
        runtime.activate_default_account(&package, || {
            checks += 1;
            checks <= 2
        }),
        Err(RuntimeError::OperationFailed)
    );

    let (packages, _slots, android) = runtime.into_parts();
    let aggregate = packages.load(&package).unwrap().unwrap();
    assert!(aggregate.is_ready());
    assert_eq!(aggregate.active_slot(), &slot);
    assert!(android.view(&package).unwrap().matches(&slot));
    assert!(!android.is_running(&package));
    assert!(android.calls()[calls_before..].iter().any(|call| matches!(
        call, AndroidCall::Apply(_, target) if target == &slot
    )));
    assert!(
        !android.calls()[calls_before..]
            .iter()
            .any(|call| matches!(call, AndroidCall::Launch(_, _)))
    );
}

#[test]
fn default_account_return_does_not_interrupt_an_account_io_lease() {
    let package = package();
    let (mut runtime, slot) = with_slot();
    runtime
        .set_default_slot(&package, Some(slot.clone()))
        .unwrap();
    let lease = runtime
        .begin_backup_io(&package, AccountIoScope::Account { slot: slot.clone() })
        .unwrap();
    assert_eq!(
        runtime.activate_default_account(&package, || true),
        Err(RuntimeError::IoBusy)
    );
    runtime.finish_backup_io(&lease.io_token).unwrap();
    assert_eq!(
        runtime.get_package(&package).unwrap().active_slot,
        SlotId::base()
    );
}
