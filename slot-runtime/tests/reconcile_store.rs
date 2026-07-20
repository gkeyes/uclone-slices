#![doc = "Fail-closed reconciliation store enumeration tests."]
#![allow(clippy::unwrap_used, reason = "validated reconciliation test fixtures")]

use std::fs;
use std::os::unix::fs::symlink;

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tempfile::TempDir;
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, SlotId, SlotView, TransactionId, UserId,
};
use uclone_slot_runtime::enrollment::EnrollmentStore;
use uclone_slot_runtime::journal::{JournalEvent, JournalStore, TransactionSpec};
use uclone_slot_runtime::lifecycle::LifecycleState;

#[allow(dead_code)]
mod support;

#[test]
fn journal_enumeration_rejects_unexpected_transaction_artifact() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let artifact = store.root().join("transactions/unexpected-artifact");
    fs::write(artifact, b"not a transaction").unwrap();

    let error = store.list().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("unexpected transaction artifact")
    );
}

#[test]
fn enrollment_enumeration_rejects_unexpected_package_artifact() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = EnrollmentStore::new(root.path()).unwrap();
    store.create(&base_package()).unwrap();
    let artifact = store.root().join("packages/README");
    fs::write(artifact, b"not an enrollment").unwrap();

    let error = store.list().unwrap_err();

    assert!(error.to_string().contains("unexpected enrollment artifact"));
}

#[test]
fn enrollment_load_binds_record_to_requested_directory_name() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = EnrollmentStore::new(root.path()).unwrap();
    let managed = base_package();
    store.create(&managed).unwrap();
    let other = PackageName::parse("com.example.other").unwrap();
    let other_dir = store.root().join("packages").join(other.as_str());
    fs::create_dir(&other_dir).unwrap();
    fs::copy(
        store
            .root()
            .join("packages/com.uclone.slotprobe/enrollment.json"),
        other_dir.join("enrollment.json"),
    )
    .unwrap();

    assert!(store.load(&other).is_err());
}

#[test]
fn journal_enumeration_rejects_hidden_and_symlink_step_artifacts() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let spec = transaction_spec("tx-store-step-artifacts");
    store.create(&spec).unwrap();
    let steps = store
        .transaction_path(spec.transaction_id())
        .unwrap()
        .join("steps");
    fs::write(steps.join(".unexpected"), b"hidden").unwrap();
    assert!(store.list().is_err());
    fs::remove_file(steps.join(".unexpected")).unwrap();
    symlink(
        steps.join("0000000000000001.json"),
        steps.join("0000000000000002.json"),
    )
    .unwrap();

    assert!(store.list().is_err());
}

#[test]
fn journal_load_rejects_prepared_spec_id_different_from_directory() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let store = JournalStore::new(root.path().join("journal")).unwrap();
    let original = transaction_spec("tx-store-outer-id");
    store.create(&original).unwrap();
    let path = store.step_path(original.transaction_id(), 1).unwrap();
    let mut wire: WireStep = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    wire.event = JournalEvent::Prepared {
        spec: Box::new(transaction_spec("tx-store-inner-id")),
    };
    wire.sha256 = digest_step(&wire);
    fs::write(path, serde_json::to_vec(&wire).unwrap()).unwrap();

    assert!(store.load(original.transaction_id()).is_err());
}

fn base_package() -> ManagedPackage {
    let base = DataInodes::new(101, 201).unwrap();
    ManagedPackage::new(
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
        AppIdentity::new(
            10_321,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            1,
            "/data/app/slotprobe/base.apk",
        )
        .unwrap(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap()
}

fn transaction_spec(id: &str) -> TransactionSpec {
    let managed = base_package();
    TransactionSpec::new(
        TransactionId::parse(id).unwrap(),
        managed,
        SlotView::new(
            SlotId::parse("work").unwrap(),
            DataInodes::new(301, 401).unwrap(),
        ),
        GateSnapshot::new(PackageEnabledState::Enabled, false),
        "boot-store-0001",
    )
    .unwrap()
}

#[derive(Serialize, Deserialize)]
struct WireStep {
    schema_version: u32,
    transaction_id: TransactionId,
    generation: u64,
    previous_sha256: Option<String>,
    event: JournalEvent,
    sha256: String,
}

fn digest_step(step: &WireStep) -> String {
    #[derive(Serialize)]
    struct UnsignedStep<'a> {
        schema_version: u32,
        transaction_id: &'a TransactionId,
        generation: u64,
        previous_sha256: Option<&'a str>,
        event: &'a JournalEvent,
    }
    let unsigned = UnsignedStep {
        schema_version: step.schema_version,
        transaction_id: &step.transaction_id,
        generation: step.generation,
        previous_sha256: step.previous_sha256.as_deref(),
        event: &step.event,
    };
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&unsigned).unwrap())
    )
}
