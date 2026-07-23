#![doc = "Independent tests for the boot emergency manifest and its secure store."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture setup must abort the individual test"
)]

use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::sync::{Arc, Barrier};
use std::thread;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{BootId, PackageName};
use uclone_slot_runtime::emergency_manifest::{
    ContainmentObligation, EmergencyManifestError, EmergencyManifestStore, EmergencyManifestV1,
    OverallDisposition, PackageContainment,
};

fn boot(value: &str) -> BootId {
    BootId::parse(value).unwrap()
}

fn package(value: &str, obligation: ContainmentObligation) -> PackageContainment {
    PackageContainment::new(PackageName::parse(value).unwrap(), obligation)
}

fn package_at(value: &str, epoch: u64, obligation: ContainmentObligation) -> PackageContainment {
    PackageContainment::with_enrollment_epoch(PackageName::parse(value).unwrap(), epoch, obligation)
}

fn store() -> (TempDir, EmergencyManifestStore) {
    let root = TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let store = EmergencyManifestStore::new(root.path()).unwrap();
    (root, store)
}

#[test]
fn derives_sorted_packages_and_strongest_overall_disposition() {
    let manifest = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![
            package("com.example.zed", ContainmentObligation::BaseRetired),
            package("com.example.alpha", ContainmentObligation::Held),
        ],
    )
    .unwrap();

    assert_eq!(manifest.overall_disposition(), OverallDisposition::Held);
    assert_eq!(
        manifest.packages().first().unwrap().package().as_str(),
        "com.example.alpha"
    );
}

#[test]
fn overall_disposition_is_not_weakened_by_a_later_not_managed_entry() {
    let manifest = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![
            package("com.example.alpha", ContainmentObligation::BaseRetired),
            package("com.example.zed", ContainmentObligation::NotManaged),
        ],
    )
    .unwrap();
    assert_eq!(
        manifest.overall_disposition(),
        OverallDisposition::BaseRetired
    );
}

#[test]
fn rejects_duplicate_unknown_and_forged_overall_records() {
    let duplicate = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![
            package("com.example.app", ContainmentObligation::Held),
            package("com.example.app", ContainmentObligation::HeldRecovery),
        ],
    )
    .unwrap_err();
    assert!(duplicate.to_string().contains("duplicate package"));

    let unknown = serde_json::from_str::<EmergencyManifestV1>(
        r#"{"schema_version":2,"boot_id":"boot-00000001","generation":1,"overall_disposition":"not_managed","packages":[],"extra":true}"#,
    )
    .unwrap_err();
    assert!(unknown.to_string().contains("unknown field"));

    let forged = serde_json::from_str::<EmergencyManifestV1>(
        r#"{"schema_version":2,"boot_id":"boot-00000001","generation":1,"overall_disposition":"held","packages":[]}"#,
    )
    .unwrap_err();
    assert!(forged.to_string().contains("overall disposition"));
}

#[test]
fn commits_monotonic_same_boot_and_allows_only_new_boot_generation_one() {
    let (_root, store) = store();
    let first = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.app", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&first).unwrap();
    let second = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package(
            "com.example.app",
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    store.commit(&second).unwrap();

    let stale = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package(
            "com.example.app",
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&stale),
        Err(EmergencyManifestError::StaleGeneration { .. })
    ));

    let wrong_new_boot = EmergencyManifestV1::new(boot("boot-00000002"), 2, Vec::new()).unwrap();
    assert!(matches!(
        store.commit(&wrong_new_boot),
        Err(EmergencyManifestError::StaleGeneration { .. })
    ));
    let new_boot = EmergencyManifestV1::new(boot("boot-00000002"), 1, Vec::new()).unwrap();
    store.commit(&new_boot).unwrap();
    assert_eq!(
        store.load_for_boot(&boot("boot-00000002")).unwrap(),
        Some(new_boot)
    );
}

#[test]
fn rejects_illegal_transition_and_stale_boot_read() {
    let (_root, store) = store();
    let first = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package(
            "com.example.app",
            ContainmentObligation::BaseRetired,
        )],
    )
    .unwrap();
    store.commit(&first).unwrap();
    let illegal = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.app",
            1,
            ContainmentObligation::NotManaged,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&illegal),
        Err(EmergencyManifestError::IllegalTransition { .. })
    ));
    assert!(matches!(
        store.load_for_boot(&boot("boot-00000002")),
        Err(EmergencyManifestError::StaleBoot { .. })
    ));
}

#[test]
fn permits_a_new_package_to_join_the_same_boot_generation() {
    let (_root, store) = store();
    let first = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.alpha", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&first).unwrap();
    let second = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![
            package("com.example.alpha", ContainmentObligation::Held),
            package("com.example.beta", ContainmentObligation::Held),
        ],
    )
    .unwrap();
    store.commit(&second).unwrap();
    assert_eq!(store.load().unwrap(), Some(second));
}

#[test]
fn requires_an_epoch_fenced_reenrollment_after_base_retired() {
    let (_root, store) = store();
    let retired = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package(
            "com.example.alpha",
            ContainmentObligation::BaseRetired,
        )],
    )
    .unwrap();
    store.commit(&retired).unwrap();

    let stale_reenrollment = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.alpha",
            1,
            ContainmentObligation::Held,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&stale_reenrollment),
        Err(EmergencyManifestError::EnrollmentEpoch { .. })
    ));

    let reenrollment = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.alpha",
            2,
            ContainmentObligation::Held,
        )],
    )
    .unwrap();
    store.commit(&reenrollment).unwrap();
    assert_eq!(store.load().unwrap(), Some(reenrollment));
}

#[test]
fn preserves_not_managed_tombstone_and_fences_reenrollment_epoch() {
    let (_root, store) = store();
    let not_managed = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package(
            "com.example.alpha",
            ContainmentObligation::NotManaged,
        )],
    )
    .unwrap();
    store.commit(&not_managed).unwrap();

    let deleted = EmergencyManifestV1::new(boot("boot-00000001"), 2, Vec::new()).unwrap();
    assert!(matches!(
        store.commit(&deleted),
        Err(EmergencyManifestError::IllegalTransition { .. })
    ));

    let stale_reenrollment = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.alpha",
            1,
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&stale_reenrollment),
        Err(EmergencyManifestError::EnrollmentEpoch { .. })
    ));

    let reenrollment = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.alpha",
            2,
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    store.commit(&reenrollment).unwrap();
    assert_eq!(store.load().unwrap(), Some(reenrollment));
}

#[test]
fn ignores_partial_temp_and_fails_closed_on_corrupt_committed_record() {
    let (root, store) = store();
    let manifest = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.app", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&manifest).unwrap();
    let temporary = root.path().join(".manifest.json.tmp-orphan");
    fs::write(&temporary, b"partial").unwrap();
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(store.load().unwrap(), Some(manifest.clone()));

    fs::write(store.manifest_path(), b"{\"partial\":true}").unwrap();
    let error = store.load().unwrap_err();
    assert!(matches!(error, EmergencyManifestError::Corrupt(_)));
}

#[test]
fn publishes_root_owned_record_with_bounded_permissions() {
    let (_root, store) = store();
    let manifest = EmergencyManifestV1::new(boot("boot-00000001"), 1, Vec::new()).unwrap();
    store.commit(&manifest).unwrap();
    let metadata = fs::symlink_metadata(store.manifest_path()).unwrap();
    assert_eq!(metadata.mode() & 0o7777, 0o600);
    assert_eq!(metadata.nlink(), 1);
    assert_eq!(
        metadata.uid(),
        fs::symlink_metadata(store.root()).unwrap().uid()
    );
}

#[test]
fn creates_a_fixed_root_owned_commit_lock() {
    let (root, _store) = store();
    let lock = root.path().join(".manifest.lock");
    let metadata = fs::symlink_metadata(lock).unwrap();
    assert_eq!(metadata.mode() & 0o7777, 0o600);
    assert_eq!(metadata.nlink(), 1);
    assert_eq!(metadata.len(), 0);
    assert_eq!(
        metadata.uid(),
        fs::symlink_metadata(root.path()).unwrap().uid()
    );
}

#[test]
fn serializes_two_same_generation_writers_with_one_winner() {
    let (_root, store) = store();
    let first = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.alpha", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&first).unwrap();
    let candidate = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package(
            "com.example.alpha",
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let left = store.clone();
    let right = store.clone();
    let left_barrier = Arc::clone(&barrier);
    let right_barrier = Arc::clone(&barrier);
    let left_candidate = candidate.clone();
    let right_candidate = candidate.clone();
    let left_thread = thread::spawn(move || {
        left_barrier.wait();
        left.commit(&left_candidate)
    });
    let right_thread = thread::spawn(move || {
        right_barrier.wait();
        right.commit(&right_candidate)
    });
    barrier.wait();
    let left_result = left_thread.join().unwrap();
    let right_result = right_thread.join().unwrap();
    assert_eq!(left_result.is_ok() as u8 + right_result.is_ok() as u8, 1);
    assert!(matches!(
        left_result.err().or(right_result.err()),
        Some(EmergencyManifestError::StaleGeneration { .. })
    ));
    assert_eq!(store.load().unwrap(), Some(candidate));
}
