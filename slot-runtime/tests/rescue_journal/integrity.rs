#![allow(
    clippy::unwrap_used,
    reason = "filesystem tampering fixtures must abort the individual test on failure"
)]

use std::fs;
use std::os::unix::fs::{PermissionsExt as _, symlink};

use tempfile::TempDir;
use uclone_slot_runtime::rescue::RescueEvent;

use super::support;

#[test]
fn rejects_digest_schema_and_json_tampering() {
    let root = TempDir::new().unwrap();
    let store = support::store(&root);
    let spec = support::spec();
    store.begin(&spec).unwrap();
    store
        .append(spec.rescue_id(), RescueEvent::GateHeld)
        .unwrap();
    let second = store.step_path(2);
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&second).unwrap()).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("schema_version".to_owned(), serde_json::json!(999));
    fs::write(&second, serde_json::to_vec(&json).unwrap()).unwrap();

    let error = store.load().unwrap_err();
    assert!(error.to_string().contains("schema"));
}

#[test]
fn rejects_unexpected_artifact_and_untrusted_mode() {
    let root = TempDir::new().unwrap();
    let store = support::store(&root);
    let spec = support::spec();
    store.begin(&spec).unwrap();
    fs::write(root.path().join("unexpected"), b"x").unwrap();
    assert!(store.load().unwrap_err().to_string().contains("artifact"));

    fs::remove_file(root.path().join("unexpected")).unwrap();
    let first = store.step_path(1);
    fs::set_permissions(&first, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(store.load().unwrap_err().to_string().contains("untrusted"));
}

#[test]
fn rejects_hard_link_and_symlink_step_artifacts() {
    let root = TempDir::new().unwrap();
    let store = support::store(&root);
    let spec = support::spec();
    store.begin(&spec).unwrap();
    let first = store.step_path(1);
    let external = TempDir::new().unwrap();
    let outside = external.path().join("linked-step");
    fs::hard_link(&first, &outside).unwrap();
    assert!(store.load().unwrap_err().to_string().contains("untrusted"));

    fs::remove_file(&outside).unwrap();
    let bytes = fs::read(&first).unwrap();
    fs::remove_file(&first).unwrap();
    let target = external.path().join("target-step");
    fs::write(&target, bytes).unwrap();
    symlink(&target, &first).unwrap();
    assert!(store.load().unwrap_err().to_string().contains("untrusted"));
}

#[test]
fn rejects_unknown_fields_and_accepts_another_valid_user_zero_package() {
    assert!(support::spec_for_package("com.example.other").is_ok());

    let root = TempDir::new().unwrap();
    let store = support::store(&root);
    let spec = support::spec();
    store.begin(&spec).unwrap();
    let first = store.step_path(1);
    let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
    json.as_object_mut()
        .unwrap()
        .insert("unexpected".to_owned(), serde_json::json!(true));
    fs::write(&first, serde_json::to_vec(&json).unwrap()).unwrap();
    let error = store.load().unwrap_err();
    assert!(error.to_string().contains("unknown field"));
}
