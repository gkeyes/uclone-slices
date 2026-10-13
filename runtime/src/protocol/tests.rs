#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use super::*;
use crate::adapters::{MemoryAndroidOps, MemoryPackageStore, MemorySlotStorage};

const CLIENT_BUILD_ID: &str = "0.2.2";

#[test]
fn shared_fixtures_cover_the_core_command_flow() {
    let package = PackageName::new("com.example.app").unwrap();
    let mut android = MemoryAndroidOps::default();
    android.install(package);
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        android,
    );
    for name in [
        "probe",
        "list_packages",
        "enroll",
        "get_package",
        "create_slot",
        "activate_slot",
        "set_launch_after_reboot",
    ] {
        let request = fixture(format!("{name}.request.json"));
        let expected = fixture(format!("{name}.response.json"));
        let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());
        assert_eq!(actual.trim(), expected.trim(), "fixture {name}");
    }
}

#[test]
fn shared_rename_fixture_covers_the_metadata_operation() {
    let mut runtime = runtime_with_slot();
    let request = fixture("rename_slot.request.json".to_owned());
    let expected = fixture("rename_slot.response.json".to_owned());

    let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

    assert_eq!(actual.trim(), expected.trim());
}

#[test]
fn shared_delete_fixture_covers_the_storage_operation() {
    let mut runtime = runtime_with_slot();
    let request = fixture("delete_slot.request.json".to_owned());
    let expected = fixture("delete_slot.response.json".to_owned());

    let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

    assert_eq!(actual.trim(), expected.trim());
}

#[test]
fn shared_unenroll_fixture_covers_the_registration_removal() {
    let mut runtime = runtime_with_slot();
    let request = fixture("unenroll.request.json".to_owned());
    let expected = fixture("unenroll.response.json".to_owned());

    let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());
    let repeated = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

    assert_eq!(actual.trim(), expected.trim());
    assert_eq!(repeated.trim(), expected.trim());
}

#[test]
fn unknown_field_is_an_invalid_request() {
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        MemoryAndroidOps::default(),
    );

    let response = handle_line(
        &mut runtime,
        CLIENT_BUILD_ID,
        r#"{"op":"probe","future":true}"#,
    );

    assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
}

#[test]
fn unenroll_rejects_fields_that_have_no_consumer() {
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        MemoryAndroidOps::default(),
    );

    let response = handle_line(
        &mut runtime,
        CLIENT_BUILD_ID,
        r#"{"op":"unenroll","package":"com.example.app","slot":"base","client_build_id":"0.2.2"}"#,
    );

    assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
}

#[test]
fn reboot_launch_setting_requires_a_boolean() {
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        MemoryAndroidOps::default(),
    );

    let response = handle_line(
        &mut runtime,
        CLIENT_BUILD_ID,
        r#"{"op":"set_launch_after_reboot","package":"com.example.app","enabled":"yes","client_build_id":"0.2.2"}"#,
    );

    assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
}

#[test]
fn rebind_rejects_invalid_certificate_digests_and_unknown_signing_fields() {
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        MemoryAndroidOps::default(),
    );

    for request in [
        r#"{"op":"rebind_package","package":"com.example.app","signing":{"kind":"lineage","sha256":["bad"]},"trust_legacy":false,"client_build_id":"0.2.2"}"#,
        r#"{"op":"rebind_package","package":"com.example.app","signing":{"kind":"lineage","sha256":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],"future":true},"trust_legacy":false,"client_build_id":"0.2.2"}"#,
    ] {
        assert_eq!(
            handle_line(&mut runtime, CLIENT_BUILD_ID, request),
            "{\"error\":{\"code\":\"invalid_request\"}}\n"
        );
    }
}

#[test]
fn reset_marker_only_accepts_the_ui_backed_true_value() {
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        MemoryAndroidOps::default(),
    );

    let response = handle_line(
        &mut runtime,
        CLIENT_BUILD_ID,
        r#"{"op":"enroll","package":"com.example.app","reset":false,"client_build_id":"0.2.2"}"#,
    );

    assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
}

#[test]
fn old_reset_enroll_is_rejected_without_deleting_slots() {
    let package = PackageName::new("com.example.app").unwrap();
    let mut android = MemoryAndroidOps::default();
    android.install(package.clone());
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        android,
    );
    runtime.enroll(package.clone(), false, None).unwrap();
    let created = runtime
        .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
        .unwrap();
    let target = created.slots[1].id.clone();
    let (packages, slots, mut android) = runtime.into_parts();
    android.change_identity(&package);
    let mut runtime = Runtime::new(packages, slots, android);
    let request = fixture("enroll_reset.request.json".to_owned());
    let expected = fixture("enroll_reset.response.json".to_owned());
    let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

    assert_eq!(actual.trim(), expected.trim());
    let (_packages, slots, _android) = runtime.into_parts();
    assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
}

#[test]
fn shared_rebind_fixture_preserves_the_existing_slot() {
    let package = PackageName::new("com.example.app").unwrap();
    let mut runtime = runtime_with_slot();
    let target = SlotId::numbered(1);
    runtime.activate_slot(&package, &target).unwrap();
    let (packages, slots, mut android) = runtime.into_parts();
    android.change_identity(&package);
    let mut runtime = Runtime::new(packages, slots, android);

    let request = fixture("rebind_package.request.json".to_owned());
    let expected = fixture("rebind_package.response.json".to_owned());
    let actual = handle_line(&mut runtime, CLIENT_BUILD_ID, request.trim());

    assert_eq!(actual.trim(), expected.trim());
    let (_packages, slots, _android) = runtime.into_parts();
    assert_eq!(slots.domain_state(&package, &target), Some((true, true)));
}

#[test]
fn probe_keeps_the_legacy_wire_shape() {
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        MemoryAndroidOps::default(),
    );

    let response = handle_line(&mut runtime, CLIENT_BUILD_ID, r#"{"op":"probe"}"#);

    assert!(response.contains("build_id"));
}

#[test]
fn every_non_probe_request_requires_the_exact_client_build_id() {
    for request in [
        r#"{"op":"list_packages"}"#,
        r#"{"op":"get_package","package":"com.example.app"}"#,
        r#"{"op":"enroll","package":"com.example.app","signing":{"kind":"lineage","sha256":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}}"#,
        r#"{"op":"list_packages","client_build_id":"0.1.8"}"#,
    ] {
        let mut runtime = Runtime::new(
            MemoryPackageStore::default(),
            MemorySlotStorage::default(),
            MemoryAndroidOps::default(),
        );

        assert_eq!(
            handle_line(&mut runtime, CLIENT_BUILD_ID, request),
            "{\"error\":{\"code\":\"invalid_request\"}}\n",
            "request: {request}",
        );
    }
}

#[test]
fn account_backup_io_is_exposed_as_a_versioned_runtime_transaction() {
    let mut runtime = runtime_with_slot();

    let response = handle_line(
        &mut runtime,
        "0.2.2",
        r#"{"op":"begin_backup_io","package":"com.example.app","scope":{"kind":"account","slot":"base"},"client_build_id":"0.2.2"}"#,
    );

    assert!(response.contains("\"io_token\""), "response: {response}");
    assert!(response.contains("\"sources\""), "response: {response}");
}

#[test]
fn account_io_status_is_queryable_without_mutating_package_state() {
    let mut runtime = runtime_with_slot();

    let response = handle_line(
        &mut runtime,
        "0.2.2",
        r#"{"op":"list_account_io_status","client_build_id":"0.2.2"}"#,
    );

    assert_eq!(response, "{\"ok\":{\"account_io\":[]}}\n");
}

#[test]
fn rejected_old_write_request_has_no_side_effect() {
    let package = PackageName::new("com.example.app").unwrap();
    let mut android = MemoryAndroidOps::default();
    android.install(package.clone());
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        android,
    );

    let response = handle_line(
        &mut runtime,
        CLIENT_BUILD_ID,
        r#"{"op":"enroll","package":"com.example.app","signing":{"kind":"lineage","sha256":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}}"#,
    );

    assert_eq!(response, "{\"error\":{\"code\":\"invalid_request\"}}\n");
    let (packages, _slots, _android) = runtime.into_parts();
    assert!(packages.load(&package).unwrap().is_none());
}

fn fixture(name: String) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../protocol/fixtures")
        .join(name);
    fs::read_to_string(path).unwrap()
}

fn runtime_with_slot() -> Runtime<MemoryPackageStore, MemorySlotStorage, MemoryAndroidOps> {
    let package = PackageName::new("com.example.app").unwrap();
    let mut android = MemoryAndroidOps::default();
    android.install(package.clone());
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        android,
    );
    runtime.enroll(package.clone(), false, None).unwrap();
    runtime
        .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
        .unwrap();
    runtime
}
