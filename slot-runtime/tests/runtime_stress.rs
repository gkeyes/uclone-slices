#![doc = "Host-only stress evidence for the paired CE and DE switch coordinator."]
#![allow(
    clippy::unwrap_used,
    reason = "validated stress fixtures fail immediately"
)]

#[path = "runtime_stress/support.rs"]
mod support;

use std::collections::BTreeSet;

use support::{new_coordinator, request};
use uclone_slot_runtime::domain::SlotId;
use uclone_slot_runtime::journal::{JournalEvent, TransactionView};
use uclone_slot_runtime::runtime::SwitchOutcome;

const ITERATIONS: usize = 1_000;

#[test]
fn fake_ce_de_switch_stress_keeps_one_mount_pair_and_valid_chain() {
    let (_root, mut runtime) = new_coordinator();
    let mut expected_slot = SlotId::base();

    for index in 0..ITERATIONS {
        let (request, target_slot) = request(runtime.backend(), index);
        assert_eq!(request.managed_package().active_slot(), &expected_slot);

        let outcome = runtime.switch(&request).unwrap();
        assert!(matches!(outcome, SwitchOutcome::Committed { .. }));
        let transaction = runtime
            .stores()
            .journal()
            .load(request.metadata().transaction_id())
            .unwrap();
        assert_eq!(transaction.view(), TransactionView::CompletedTarget);
        assert_eq!(transaction.steps().len(), 9);
        assert!(matches!(
            transaction
                .steps()
                .last()
                .map(uclone_slot_runtime::journal::JournalStep::event),
            Some(JournalEvent::Completed)
        ));
        assert!(!runtime.backend().gate_held);
        expected_slot = target_slot;
    }

    let transactions = runtime.stores().journal().list().unwrap();
    assert_eq!(transactions.len(), ITERATIONS);
    let generations: BTreeSet<_> = transactions
        .iter()
        .map(|transaction| transaction.steps().last().unwrap().generation())
        .collect();
    assert_eq!(generations, BTreeSet::from([9_u64]));
    let latest = runtime
        .stores()
        .registry()
        .latest(runtime.backend().managed().package_name())
        .unwrap()
        .unwrap();
    assert_eq!(latest.active_slot(), &expected_slot);
    assert_eq!(runtime.backend().current.slot_id(), &expected_slot);
    assert_eq!(runtime.backend().lease_retired, ITERATIONS);
    assert_eq!(runtime.backend().mount_apply_calls, ITERATIONS);
    assert_eq!(runtime.backend().active_mounts, 2);
    assert_eq!(runtime.backend().peak_mounts, 2);
}
