#![allow(
    clippy::unwrap_used,
    reason = "test fixtures are intentionally compact"
)]

use std::fs;
use std::os::unix::fs::symlink;

use tempfile::TempDir;

use super::{CommitProof, EnrollmentAttemptPhase, EnrollmentAttemptStore, RetirementProof};
use crate::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, SlotId, SlotView, UserId,
};
use crate::lifecycle::LifecycleState;

const PACKAGE: &str = "com.uclone.slotprobe";
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn pending_attempt_is_enumerable_without_enrollment_store_and_survives_restart() {
    let root = TempDir::new().unwrap();
    let key = key();
    let gate = GateSnapshot::new(PackageEnabledState::Enabled, true);
    let store = EnrollmentAttemptStore::new(root.path()).unwrap();

    let created = store.create_pending(&key, gate).unwrap();
    assert_eq!(created.phase(), EnrollmentAttemptPhase::Pending);
    assert_eq!(store.list().unwrap().len(), 1);
    drop(store);

    let restarted = EnrollmentAttemptStore::new(root.path()).unwrap();
    let loaded = restarted.load(&key).unwrap().unwrap();
    assert_eq!(loaded.gate_snapshot(), gate);
    assert_eq!(loaded.phase(), EnrollmentAttemptPhase::Pending);
}

#[test]
fn tampered_generation_and_unknown_schema_fail_closed() {
    let root = TempDir::new().unwrap();
    let key = key();
    let store = EnrollmentAttemptStore::new(root.path()).unwrap();
    store
        .create_pending(&key, GateSnapshot::new(PackageEnabledState::Default, false))
        .unwrap();
    let path = root
        .path()
        .join("attempts")
        .join(PACKAGE)
        .join("generations/0000000000000001.json");
    let mut bytes = fs::read(&path).unwrap();
    *bytes.first_mut().unwrap() = b'!';
    fs::write(&path, bytes).unwrap();
    assert!(store.load(&key).is_err());

    let unknown = root.path().join("attempts/unknown");
    fs::create_dir_all(&unknown).unwrap();
    assert!(store.list().is_err());
}

#[test]
fn symlink_and_partial_generation_artifacts_fail_closed() {
    let root = TempDir::new().unwrap();
    let key = key();
    let store = EnrollmentAttemptStore::new(root.path()).unwrap();
    store
        .create_pending(&key, GateSnapshot::new(PackageEnabledState::Default, false))
        .unwrap();
    let generations = root
        .path()
        .join("attempts/com.uclone.slotprobe/generations");
    symlink(
        generations.join("0000000000000001.json"),
        generations.join("0000000000000002.json"),
    )
    .unwrap();
    assert!(store.load(&key).is_err());
}

#[test]
fn markerless_commit_is_non_authoritative_recovery_required() {
    let root = TempDir::new().unwrap();
    let key = key();
    let gate = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let store = EnrollmentAttemptStore::new(root.path()).unwrap();
    store.create_pending(&key, gate).unwrap();
    let proof = CommitProof::new(managed(), DIGEST, DIGEST, DIGEST).unwrap();
    store.commit(&key, proof).unwrap();
    fs::remove_file(
        root.path()
            .join("attempts/com.uclone.slotprobe/commit.marker"),
    )
    .unwrap();

    let loaded = store.load(&key).unwrap().unwrap();
    assert_eq!(loaded.phase(), EnrollmentAttemptPhase::RecoveryRequired);
    assert!(!loaded.is_authoritative());
}

#[test]
fn commit_requires_all_publication_digests_and_retire_requires_exact_proof() {
    let root = TempDir::new().unwrap();
    let key = key();
    let gate = GateSnapshot::new(PackageEnabledState::Enabled, false);
    let store = EnrollmentAttemptStore::new(root.path()).unwrap();
    store.create_pending(&key, gate).unwrap();
    let incomplete = CommitProof::from_managed(managed()).unwrap();
    assert!(store.commit(&key, incomplete).is_err());

    let proof = CommitProof::new(managed(), DIGEST, DIGEST, DIGEST).unwrap();
    store.commit(&key, proof).unwrap();
    assert!(
        store
            .retire(&key, RetirementProof::verified(gate, true, false))
            .is_err()
    );
    store.retire(&key, RetirementProof::new(gate)).unwrap();
    assert!(store.load(&key).unwrap().is_none());
}

#[test]
fn pending_attempt_can_abort_without_a_commit_marker() {
    let root = TempDir::new().unwrap();
    let key = key();
    let gate = GateSnapshot::new(PackageEnabledState::Default, false);
    let store = EnrollmentAttemptStore::new(root.path()).unwrap();
    store.create_pending(&key, gate).unwrap();

    store
        .abort_pending(&key, RetirementProof::new(gate))
        .unwrap();

    assert!(store.load(&key).unwrap().is_none());
}

fn key() -> PackageKey {
    PackageKey::new(PackageName::parse(PACKAGE).unwrap(), UserId::PRIMARY)
}

fn managed() -> ManagedPackage {
    let inodes = DataInodes::new(101, 201).unwrap();
    ManagedPackage::new(
        key(),
        AppIdentity::new(10_321, DIGEST, 1, "/data/app/slotprobe/base.apk").unwrap(),
        inodes,
        SlotView::new(SlotId::base(), inodes),
        LifecycleState::Normal,
    )
    .unwrap()
}
