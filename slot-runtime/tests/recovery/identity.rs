use super::support;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{AppIdentity, CommitNonce, DataInodes, SlotId, SlotView};
use uclone_slot_runtime::journal::{JournalEvent, JournalStore};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::recovery::{RecoveryDecision, decide_recovery};
use uclone_slot_runtime::registry::{PackageRevision, RegistryStore};
#[test]
fn rejects_previous_view_from_a_different_base_anchor() {
    let root = TempDir::new().unwrap();
    crate::support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let work = DataInodes::new(301, 401).unwrap();
    let base = DataInodes::new(101, 201).unwrap();
    let pending = support::transaction_spec(support::TransactionFixture::new(
        "tx-recovery-0003",
        support::TransactionViews::new(
            base,
            SlotView::new(SlotId::parse("work").unwrap(), work),
            SlotView::new(
                SlotId::parse("personal").unwrap(),
                DataInodes::new(501, 601).unwrap(),
            ),
        ),
        "boot-recovery-0003",
    ));
    journal.create(&pending).unwrap();
    journal
        .append(pending.transaction_id(), JournalEvent::GateHeld)
        .unwrap();

    let different_base = DataInodes::new(111, 211).unwrap();
    let old_transaction = support::transaction_spec(support::TransactionFixture::new(
        "tx-recovery-older",
        support::TransactionViews::new(
            different_base,
            SlotView::new(SlotId::base(), different_base),
            SlotView::new(SlotId::parse("work").unwrap(), work),
        ),
        "boot-recovery-old",
    ));
    registry
        .append(
            &PackageRevision::committed(
                &old_transaction,
                different_base,
                CommitNonce::parse("nonce-recovery-old").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    let transaction = journal.load(pending.transaction_id()).unwrap();
    let latest = registry.latest(pending.package_name()).unwrap().unwrap();

    assert_eq!(
        decide_recovery(&transaction, Some(&latest)),
        RecoveryDecision::RecoveryRequired,
    );
}

#[test]
fn requires_manual_recovery_when_registry_identity_does_not_match_journal() {
    let root = TempDir::new().unwrap();
    crate::support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let pending = support::transaction_spec(support::TransactionFixture::new(
        "tx-00000002",
        support::TransactionViews::new(
            DataInodes::new(101, 201).unwrap(),
            SlotView::new(SlotId::base(), DataInodes::new(101, 201).unwrap()),
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(301, 401).unwrap(),
            ),
        ),
        "boot-00000002",
    ));
    journal.create(&pending).unwrap();
    journal
        .append(pending.transaction_id(), JournalEvent::GateHeld)
        .unwrap();

    let changed = support::transaction_spec_with_contract(
        support::TransactionFixture::new(
            "tx-recovery-identity",
            support::TransactionViews::new(
                pending.base_inodes(),
                SlotView::new(SlotId::base(), pending.base_inodes()),
                SlotView::new(SlotId::parse("work").unwrap(), pending.target_inodes()),
            ),
            "boot-recovery-identity",
        )
        .with_contract(support::PackageContract::new(
            AppIdentity::new(
                10_321,
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                1,
                "/data/app/slotprobe/base.apk",
            )
            .unwrap(),
            LifecycleState::Normal,
        )),
    );
    registry
        .append(
            &PackageRevision::committed(
                &changed,
                changed.base_inodes(),
                CommitNonce::parse("commit-recovery-identity").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    let transaction = journal.load(pending.transaction_id()).unwrap();
    let latest = registry.latest(pending.package_name()).unwrap().unwrap();

    assert_eq!(
        decide_recovery(&transaction, Some(&latest)),
        RecoveryDecision::RecoveryRequired,
    );
}

#[test]
fn requires_manual_recovery_when_registry_lifecycle_does_not_match_journal() {
    let root = TempDir::new().unwrap();
    crate::support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let pending = support::transaction_spec(support::TransactionFixture::new(
        "tx-00000002",
        support::TransactionViews::new(
            DataInodes::new(101, 201).unwrap(),
            SlotView::new(SlotId::base(), DataInodes::new(101, 201).unwrap()),
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(301, 401).unwrap(),
            ),
        ),
        "boot-00000002",
    ));
    journal.create(&pending).unwrap();
    journal
        .append(pending.transaction_id(), JournalEvent::GateHeld)
        .unwrap();

    let changed = support::transaction_spec_with_contract(
        support::TransactionFixture::new(
            "tx-recovery-lifecycle",
            support::TransactionViews::new(
                pending.base_inodes(),
                SlotView::new(SlotId::base(), pending.base_inodes()),
                SlotView::new(SlotId::parse("work").unwrap(), pending.target_inodes()),
            ),
            "boot-recovery-lifecycle",
        )
        .with_contract(support::PackageContract::new(
            pending.identity().clone(),
            LifecycleState::LifecycleDrifted,
        )),
    );
    registry
        .append(
            &PackageRevision::committed(
                &changed,
                changed.base_inodes(),
                CommitNonce::parse("commit-recovery-lifecycle").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    let transaction = journal.load(pending.transaction_id()).unwrap();
    let latest = registry.latest(pending.package_name()).unwrap().unwrap();

    assert_eq!(
        decide_recovery(&transaction, Some(&latest)),
        RecoveryDecision::RecoveryRequired,
    );
}

#[test]
fn keeps_gate_when_completed_rollback_registry_contract_does_not_match() {
    let root = TempDir::new().unwrap();
    crate::support::secure_temp_dir(&root);
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let pending = support::transaction_spec(support::TransactionFixture::new(
        "tx-00000002",
        support::TransactionViews::new(
            DataInodes::new(101, 201).unwrap(),
            SlotView::new(SlotId::base(), DataInodes::new(101, 201).unwrap()),
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(301, 401).unwrap(),
            ),
        ),
        "boot-00000002",
    ));
    journal.create(&pending).unwrap();
    for event in [
        JournalEvent::GateHeld,
        JournalEvent::ProcessesQuiesced,
        JournalEvent::Applying,
        JournalEvent::RollingBack,
        JournalEvent::RolledBack,
        JournalEvent::GateReleased,
        JournalEvent::Completed,
    ] {
        journal.append(pending.transaction_id(), event).unwrap();
    }

    let changed = support::transaction_spec_with_contract(
        support::TransactionFixture::new(
            "tx-recovery-completed-rollback",
            support::TransactionViews::new(
                pending.base_inodes(),
                SlotView::new(SlotId::base(), pending.base_inodes()),
                SlotView::new(SlotId::parse("work").unwrap(), pending.target_inodes()),
            ),
            "boot-recovery-completed",
        )
        .with_contract(support::PackageContract::new(
            pending.identity().clone(),
            LifecycleState::LifecycleDrifted,
        )),
    );
    registry
        .append(
            &PackageRevision::committed(
                &changed,
                changed.base_inodes(),
                CommitNonce::parse("nonce-recovery-completed").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    let transaction = journal.load(pending.transaction_id()).unwrap();
    let latest = registry.latest(pending.package_name()).unwrap().unwrap();

    assert_eq!(
        decide_recovery(&transaction, Some(&latest)),
        RecoveryDecision::RecoveryRequired,
    );
}
