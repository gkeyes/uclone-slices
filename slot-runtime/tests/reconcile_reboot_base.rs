#![doc = "Reconciles a committed Preview slot after Android reset the live view to base."]
#![allow(
    dead_code,
    missing_docs,
    unreachable_pub,
    clippy::unwrap_used,
    reason = "validated reconciliation test fixtures"
)]

#[path = "reconcile/support.rs"]
mod reconcile_support;

use reconcile_support::{
    FakeBackend, TestStores, base_package, publish_target, transaction, work_view,
};
use uclone_slot_runtime::domain::SlotId;
use uclone_slot_runtime::reconcile::ReconcileOutcome;

#[test]
fn restores_committed_slot_from_native_base_after_reboot() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let committed = transaction("tx-reconcile-native-base");
    publish_target(&stores.registry, &committed);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.strict_current_proof = true;
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert_eq!(
        unlocked.results().first().unwrap().outcome(),
        &ReconcileOutcome::RestoredSlot(SlotId::parse("work").unwrap()),
    );
    assert_eq!(reconciler.backend().current, work_view());
    assert_eq!(reconciler.backend().apply_calls, 1);
    assert!(!reconciler.backend().gate_held);
}

#[test]
fn already_verified_committed_slot_is_not_reapplied() {
    let managed = base_package();
    let stores = TestStores::new(&managed);
    let committed = transaction("tx-reconcile-already-active");
    publish_target(&stores.registry, &committed);
    let mut backend = FakeBackend::rebooted(&managed);
    backend.current = work_view();
    backend.strict_current_proof = true;
    let mut reconciler = stores.reconciler(backend);
    reconciler.early_boot().unwrap();

    let unlocked = reconciler.reconcile_unlocked().unwrap();

    assert!(matches!(
        unlocked.results().first().unwrap().outcome(),
        ReconcileOutcome::RestoredSlot(slot) if slot == &SlotId::parse("work").unwrap()
    ));
    assert_eq!(reconciler.backend().apply_calls, 0);
    assert!(!reconciler.backend().gate_held);
}
