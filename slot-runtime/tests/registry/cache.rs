use super::support;

use std::fs;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{CommitNonce, DataInodes, SlotId, SlotView};
use uclone_slot_runtime::journal::TransactionSpec;
use uclone_slot_runtime::registry::{PackageRevision, RegistryError, RegistryStore};

fn spec(
    transaction: &str,
    previous: SlotView,
    target: SlotView,
    base: DataInodes,
) -> TransactionSpec {
    support::transaction_spec(support::TransactionFixture::new(
        transaction,
        support::TransactionViews::new(base, previous, target),
        "boot-registry-cache",
    ))
}

fn append(store: &RegistryStore, spec: &TransactionSpec, nonce: &str, base: DataInodes) {
    let draft = PackageRevision::committed(spec, base, CommitNonce::parse(nonce).unwrap()).unwrap();
    store.append(&draft).unwrap();
}

#[test]
fn hot_append_rejects_same_size_historical_revision_tampering() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = RegistryStore::new(root.path().join("registry")).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let work = DataInodes::new(300, 400).unwrap();
    let first = spec(
        "tx-registry-cache-1",
        SlotView::new(SlotId::base(), base),
        SlotView::new(SlotId::parse("work").unwrap(), work),
        base,
    );
    let second = spec(
        "tx-registry-cache-2",
        SlotView::new(SlotId::parse("work").unwrap(), work),
        SlotView::new(SlotId::base(), base),
        base,
    );
    append(&store, &first, "nonce-registry-cache-1", base);
    append(&store, &second, "nonce-registry-cache-2", base);
    let revisions = root
        .path()
        .join("registry/packages/com.uclone.slotprobe/revisions");
    let first_path = revisions.join("0000000000000001.json");
    let mut bytes = fs::read(&first_path).unwrap();
    let digest = b"\"sha256\":\"";
    let offset = bytes
        .windows(digest.len())
        .position(|window| window == digest)
        .unwrap()
        + digest.len();
    let byte = bytes.get_mut(offset).unwrap();
    *byte = if *byte == b'a' { b'b' } else { b'a' };
    let original_len = bytes.len();
    fs::write(&first_path, &bytes).unwrap();
    assert_eq!(
        fs::metadata(&first_path).unwrap().len(),
        u64::try_from(original_len).unwrap()
    );
    let third = spec(
        "tx-registry-cache-3",
        SlotView::new(SlotId::base(), base),
        SlotView::new(SlotId::parse("work").unwrap(), work),
        base,
    );
    let draft = PackageRevision::committed(
        &third,
        base,
        CommitNonce::parse("nonce-registry-cache-3").unwrap(),
    )
    .unwrap();

    let error = store.append(&draft).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("cached chain changed"));
    assert!(!revisions.join("0000000000000003.json").exists());
}
