use super::support;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{CommitNonce, DataInodes, SlotId, SlotView};
use uclone_slot_runtime::journal::TransactionSpec;
use uclone_slot_runtime::registry::{PackageRevision, RegistryStore};

fn spec(
    transaction_id: &str,
    base_inodes: DataInodes,
    previous_slot: SlotId,
    target_slot: SlotId,
    previous_inodes: DataInodes,
    target_inodes: DataInodes,
) -> TransactionSpec {
    support::transaction_spec(support::TransactionFixture::new(
        transaction_id,
        support::TransactionViews::new(
            base_inodes,
            SlotView::new(previous_slot, previous_inodes),
            SlotView::new(target_slot, target_inodes),
        ),
        "boot-registry-0001",
    ))
}

#[test]
fn appends_multiple_revisions_without_changing_base_anchor() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = RegistryStore::new(root.path().join("registry")).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let work = DataInodes::new(300, 400).unwrap();
    let personal = DataInodes::new(500, 600).unwrap();
    let first = spec(
        "tx-registry-0001",
        base,
        SlotId::base(),
        SlotId::parse("work").unwrap(),
        base,
        work,
    );
    let second = spec(
        "tx-registry-0002",
        base,
        SlotId::parse("work").unwrap(),
        SlotId::parse("personal").unwrap(),
        work,
        personal,
    );

    store
        .append(
            &PackageRevision::committed(
                &first,
                base,
                CommitNonce::parse("nonce-registry-0001").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    store
        .append(
            &PackageRevision::committed(
                &second,
                base,
                CommitNonce::parse("nonce-registry-0002").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    let latest = store.latest(first.package_name()).unwrap().unwrap();
    assert_eq!(latest.base_inodes(), base);
    assert_eq!(latest.active_slot(), second.target_slot());
    assert_eq!(latest.active_inodes(), personal);
}

#[test]
fn rejects_first_revision_that_does_not_start_from_base() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = RegistryStore::new(root.path().join("registry")).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let work = DataInodes::new(300, 400).unwrap();
    let personal = DataInodes::new(500, 600).unwrap();
    let invalid = spec(
        "tx-registry-0003",
        base,
        SlotId::parse("work").unwrap(),
        SlotId::parse("personal").unwrap(),
        work,
        personal,
    );
    let draft = PackageRevision::committed(
        &invalid,
        base,
        CommitNonce::parse("nonce-registry-0003").unwrap(),
    )
    .unwrap();

    let error = store.append(&draft).unwrap_err();

    assert!(error.to_string().contains("first revision"));
}

#[test]
fn rejects_base_slot_with_non_base_inode_pair() {
    let base = DataInodes::new(100, 200).unwrap();
    let work = DataInodes::new(300, 400).unwrap();
    let managed = support::managed_with_contract(support::ManagedFixture::new(
        base,
        SlotView::new(SlotId::parse("work").unwrap(), work),
        support::PackageContract::new(
            uclone_slot_runtime::domain::AppIdentity::new(
                10_321,
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                1,
                "/data/app/slotprobe/base.apk",
            )
            .unwrap(),
            uclone_slot_runtime::lifecycle::LifecycleState::Normal,
        ),
    ));
    let error = uclone_slot_runtime::journal::TransactionSpec::new(
        uclone_slot_runtime::domain::TransactionId::parse("tx-registry-0004").unwrap(),
        managed,
        SlotView::new(SlotId::base(), DataInodes::new(999, 1_000).unwrap()),
        uclone_slot_runtime::domain::GateSnapshot::new(
            uclone_slot_runtime::domain::PackageEnabledState::Default,
            false,
        ),
        "boot-registry-0004",
    )
    .unwrap_err();

    assert!(error.to_string().contains("base inode anchor"));
}
