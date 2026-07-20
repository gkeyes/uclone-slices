#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

use super::{PackageRevision, RegistryError, chain};
use crate::domain::{
    AppIdentity, CommitNonce, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState,
    PackageKey, PackageName, SlotId, SlotView, TransactionId, UserId,
};
use crate::journal::TransactionSpec;
use crate::lifecycle::LifecycleState;

const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn spec(
    transaction_id: &str,
    base: DataInodes,
    previous: SlotView,
    target: SlotView,
) -> TransactionSpec {
    TransactionSpec::new(
        TransactionId::parse(transaction_id).unwrap(),
        ManagedPackage::new(
            PackageKey::new(
                PackageName::parse("com.uclone.slotprobe").unwrap(),
                UserId::PRIMARY,
            ),
            AppIdentity::new(10_321, SIGNATURE, 1, "/data/app/slotprobe/base.apk").unwrap(),
            base,
            previous,
            LifecycleState::Normal,
        )
        .unwrap(),
        target,
        GateSnapshot::new(PackageEnabledState::Default, false),
        "boot-registry-unit",
    )
    .unwrap()
}

fn first_draft() -> PackageRevision {
    let base = DataInodes::new(100, 200).unwrap();
    PackageRevision::committed(
        &spec(
            "tx-registry-unit-1",
            base,
            SlotView::new(SlotId::base(), base),
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(300, 400).unwrap(),
            ),
        ),
        base,
        CommitNonce::parse("nonce-registry-unit-1").unwrap(),
    )
    .unwrap()
}

#[test]
fn rejects_digest_valid_revision_with_invalid_slot_shape() {
    let mut revision = chain::publish(&first_draft(), None).unwrap();
    revision.active_inodes = revision.base_inodes;
    revision.sha256 = chain::revision_digest(&revision).unwrap();

    let error = chain::verify(&revision, None).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("base inode anchor"));
}

#[test]
fn rejects_generation_overflow_before_publishing_revision() {
    let mut previous = chain::publish(&first_draft(), None).unwrap();
    previous.generation = u64::MAX;
    let base = previous.base_inodes;
    let next = PackageRevision::committed(
        &spec(
            "tx-registry-unit-2",
            base,
            SlotView::new(previous.active_slot.clone(), previous.active_inodes),
            SlotView::new(
                SlotId::parse("personal").unwrap(),
                DataInodes::new(500, 600).unwrap(),
            ),
        ),
        base,
        CommitNonce::parse("nonce-registry-unit-2").unwrap(),
    )
    .unwrap();

    let error = chain::publish(&next, Some(&previous)).unwrap_err();

    assert!(error.to_string().contains("generation overflow"));
}
