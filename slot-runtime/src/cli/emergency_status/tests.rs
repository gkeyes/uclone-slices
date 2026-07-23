#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "fixture setup must fail the individual unit test"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::TempDir;

use super::*;
use crate::emergency_manifest::{
    ContainmentObligation, EmergencyManifestV1, PackageContainment, RuntimeOwnerRole,
};

fn manifest_root() -> (TempDir, std::path::PathBuf) {
    let parent = TempDir::new().unwrap();
    fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let root = parent.path().join("emergency-manifest");
    (parent, root)
}

fn boot(value: &str) -> BootId {
    BootId::parse(value).unwrap()
}

fn package(value: &str, obligation: ContainmentObligation) -> PackageContainment {
    PackageContainment::new(
        crate::domain::PackageName::parse(value).unwrap(),
        obligation,
    )
}

fn field<'a>(value: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    value.pointer(path).expect("fixture field")
}

#[test]
fn missing_root_is_reported_without_creating_any_artifact() {
    let (_parent, root) = manifest_root();
    let result = load_snapshot(
        &root,
        &root.join("runtime.lock"),
        &boot("boot-current"),
        0,
        32,
        |_, _| panic!("owner verifier must not run"),
    );

    assert!(matches!(
        result,
        Err(EmergencyStatusError::Manifest(
            EmergencyManifestError::MissingRoot { .. }
        ))
    ));
    assert!(!root.exists());
}

#[test]
fn strict_snapshot_is_sorted_paginated_and_revalidates_owner() {
    let (_parent, root) = manifest_root();
    let store = EmergencyManifestStore::new(&root).unwrap();
    let current_boot = boot("boot-current");
    let proof = RuntimeOwnerProof::new(
        current_boot.clone(),
        17,
        29,
        RuntimeOwnerRole::Ucloned,
        31,
        37,
    );
    let initial = EmergencyManifestV1::new(
        current_boot.clone(),
        1,
        vec![
            package("org.example.zeta", ContainmentObligation::Held),
            package("com.example.alpha", ContainmentObligation::Held),
        ],
    )
    .unwrap();
    store.commit(&initial).unwrap();
    let manifest = EmergencyManifestV1::with_context(
        current_boot.clone(),
        2,
        crate::emergency_manifest::DiscoveryIntegrity::Complete,
        Some(proof.clone()),
        vec![
            package("org.example.zeta", ContainmentObligation::Held),
            package("com.example.alpha", ContainmentObligation::RuntimeOwned),
        ],
    )
    .unwrap();
    store.commit(&manifest).unwrap();
    let lock = root.join("runtime.lock");

    let snapshot = load_snapshot(&root, &lock, &current_boot, 0, 1, |path, actual| {
        assert_eq!(path, lock);
        assert_eq!(actual, &proof);
        Ok(RuntimeOwnerVerdict::Valid)
    })
    .unwrap();
    let frame = EmergencyStatusFrame::success("emergency-test", snapshot);
    let value = serde_json::to_value(frame).unwrap();

    assert_eq!(field(&value, "/status"), "ok");
    assert_eq!(field(&value, "/boot_id"), "boot-current");
    assert_eq!(field(&value, "/generation"), 2);
    assert_eq!(field(&value, "/owner_verdict"), "valid");
    assert_eq!(field(&value, "/page/cursor"), 0);
    assert_eq!(field(&value, "/page/next"), 1);
    assert_eq!(field(&value, "/page/total"), 2);
    assert_eq!(
        field(&value, "/page/entries/0/package"),
        "com.example.alpha"
    );
    assert!(serde_json::to_vec(&value).unwrap().len() + 1 < crate::protocol::MAX_FRAME_SIZE);
}

#[test]
fn stale_boot_and_invalid_owner_never_produce_a_snapshot() {
    let (_parent, root) = manifest_root();
    let store = EmergencyManifestStore::new(&root).unwrap();
    let manifest = EmergencyManifestV1::new(
        boot("boot-previous"),
        1,
        vec![package("com.example.preview", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&manifest).unwrap();
    let stale = load_snapshot(
        &root,
        &root.join("runtime.lock"),
        &boot("boot-current"),
        0,
        32,
        |_, _| panic!("owner verifier must not run"),
    );
    assert!(matches!(stale, Err(EmergencyStatusError::StaleBoot { .. })));

    let root_two = root.with_file_name("emergency-manifest-two");
    let store_two = EmergencyManifestStore::new(&root_two).unwrap();
    let current_boot = boot("boot-current");
    let proof = RuntimeOwnerProof::new(
        current_boot.clone(),
        17,
        29,
        RuntimeOwnerRole::Ucloned,
        31,
        37,
    );
    let initial = EmergencyManifestV1::new(
        current_boot.clone(),
        1,
        vec![package("com.example.preview", ContainmentObligation::Held)],
    )
    .unwrap();
    store_two.commit(&initial).unwrap();
    let owned = EmergencyManifestV1::with_context(
        current_boot.clone(),
        2,
        crate::emergency_manifest::DiscoveryIntegrity::Complete,
        Some(proof),
        vec![package(
            "com.example.preview",
            ContainmentObligation::RuntimeOwned,
        )],
    )
    .unwrap();
    store_two.commit(&owned).unwrap();
    let invalid = load_snapshot(
        &root_two,
        &root_two.join("runtime.lock"),
        &current_boot,
        0,
        32,
        |_, _| {
            Ok(RuntimeOwnerVerdict::Invalid(
                RuntimeOwnerMismatch::ProcessStartTicks,
            ))
        },
    );
    assert!(matches!(
        invalid,
        Err(EmergencyStatusError::OwnerInvalid(
            RuntimeOwnerMismatch::ProcessStartTicks
        ))
    ));
}

#[test]
fn pagination_rejects_a_cursor_beyond_the_validated_package_set() {
    let (_parent, root) = manifest_root();
    let store = EmergencyManifestStore::new(&root).unwrap();
    let manifest = EmergencyManifestV1::new(
        boot("boot-current"),
        1,
        vec![package("com.example.preview", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&manifest).unwrap();

    let result = load_snapshot(
        &root,
        &root.join("runtime.lock"),
        manifest.boot_id(),
        2,
        1,
        |_, _| panic!("owner verifier must not run"),
    );
    assert!(matches!(
        result,
        Err(EmergencyStatusError::CursorOutOfRange {
            cursor: 2,
            total: 1
        })
    ));
}

#[test]
fn failure_frame_is_explicitly_fail_closed_and_bounded() {
    let error = EmergencyStatusError::OwnerMissing;
    let frame = EmergencyStatusFrame::failure("emergency-test", &error);
    let encoded = serde_json::to_vec(&frame).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&encoded).unwrap();

    assert_eq!(field(&value, "/status"), "error");
    assert_eq!(field(&value, "/error_code"), "owner_missing");
    assert_eq!(field(&value, "/discovery_integrity"), "untrusted");
    assert_eq!(field(&value, "/overall_disposition"), "held_recovery");
    assert_eq!(field(&value, "/owner_verdict"), "missing");
    assert!(encoded.len() + 1 < crate::protocol::MAX_FRAME_SIZE);
}
