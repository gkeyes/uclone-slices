use super::support;

use std::fs;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{AppIdentity, CommitNonce, DataInodes, SlotId, SlotView};
use uclone_slot_runtime::journal::TransactionSpec;
use uclone_slot_runtime::lifecycle::LifecycleState;
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

fn spec_with_contract(
    transaction_id: &str,
    base: DataInodes,
    previous: SlotView,
    target: SlotView,
    identity: AppIdentity,
    lifecycle_state: LifecycleState,
) -> TransactionSpec {
    support::transaction_spec_with_contract(
        support::TransactionFixture::new(
            transaction_id,
            support::TransactionViews::new(base, previous, target),
            "boot-registry-0001",
        )
        .with_contract(support::PackageContract::new(identity, lifecycle_state)),
    )
}

#[test]
fn rejects_tampered_revision_digest() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = RegistryStore::new(root.path().join("registry")).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let first = spec(
        "tx-registry-0005",
        base,
        SlotId::base(),
        SlotId::parse("work").unwrap(),
        base,
        DataInodes::new(300, 400).unwrap(),
    );
    store
        .append(
            &PackageRevision::committed(
                &first,
                base,
                CommitNonce::parse("nonce-registry-0005").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let path = root
        .path()
        .join("registry/packages/com.uclone.slotprobe/revisions/0000000000000001.json");
    let mut bytes = fs::read(&path).unwrap();
    let marker = bytes.iter().position(|byte| *byte == b'w').unwrap();
    if let Some(byte) = bytes.get_mut(marker) {
        *byte = b'W';
    }
    fs::write(path, bytes).unwrap();

    let error = store.latest(first.package_name()).unwrap_err();

    assert!(error.to_string().contains("digest"));
}

#[test]
fn rejects_identity_change_in_same_registry_stream() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = RegistryStore::new(root.path().join("registry")).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let work = DataInodes::new(300, 400).unwrap();
    let first = spec(
        "tx-registry-0006",
        base,
        SlotId::base(),
        SlotId::parse("work").unwrap(),
        base,
        work,
    );
    store
        .append(
            &PackageRevision::committed(
                &first,
                base,
                CommitNonce::parse("nonce-registry-0006").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    let changed = spec_with_contract(
        "tx-registry-0007",
        base,
        SlotView::new(SlotId::parse("work").unwrap(), work),
        SlotView::new(
            SlotId::parse("personal").unwrap(),
            DataInodes::new(500, 600).unwrap(),
        ),
        AppIdentity::new(
            10_321,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            1,
            "/data/app/slotprobe/base.apk",
        )
        .unwrap(),
        LifecycleState::Normal,
    );
    let draft = PackageRevision::committed(
        &changed,
        base,
        CommitNonce::parse("nonce-registry-0007").unwrap(),
    )
    .unwrap();

    let error = store.append(&draft).unwrap_err();

    assert!(error.to_string().contains("stream identity"));
}

#[test]
fn rejects_lifecycle_change_in_same_registry_stream() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = RegistryStore::new(root.path().join("registry")).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let work = DataInodes::new(300, 400).unwrap();
    let first = spec(
        "tx-registry-0008",
        base,
        SlotId::base(),
        SlotId::parse("work").unwrap(),
        base,
        work,
    );
    store
        .append(
            &PackageRevision::committed(
                &first,
                base,
                CommitNonce::parse("nonce-registry-0008").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();

    let changed = spec_with_contract(
        "tx-registry-0009",
        base,
        SlotView::new(SlotId::parse("work").unwrap(), work),
        SlotView::new(
            SlotId::parse("personal").unwrap(),
            DataInodes::new(500, 600).unwrap(),
        ),
        AppIdentity::new(
            10_321,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            1,
            "/data/app/slotprobe/base.apk",
        )
        .unwrap(),
        LifecycleState::LifecycleDrifted,
    );
    let draft = PackageRevision::committed(
        &changed,
        base,
        CommitNonce::parse("nonce-registry-0009").unwrap(),
    )
    .unwrap();

    let error = store.append(&draft).unwrap_err();

    assert!(error.to_string().contains("stream identity"));
}

#[test]
fn rejects_schema_version_tampering_before_digest_check() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = RegistryStore::new(root.path().join("registry")).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let first = spec(
        "tx-registry-0010",
        base,
        SlotId::base(),
        SlotId::parse("work").unwrap(),
        base,
        DataInodes::new(300, 400).unwrap(),
    );
    store
        .append(
            &PackageRevision::committed(
                &first,
                base,
                CommitNonce::parse("nonce-registry-0010").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let path = root
        .path()
        .join("registry/packages/com.uclone.slotprobe/revisions/0000000000000001.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("schema_version".to_owned(), serde_json::Value::from(1_u64));
    fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();

    let error = store.latest(first.package_name()).unwrap_err();

    assert!(error.to_string().contains("unsupported schema"));
}
