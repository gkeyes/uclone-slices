#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::PathBuf;

use super::*;
use crate::adapters::{MemoryAndroidOps, MemoryPackageStore, MemorySlotStorage};

const CLIENT_BUILD_ID: &str = "0.3.2";

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
        r#"{"op":"unenroll","package":"com.example.app","slot":"base","client_build_id":"0.3.2"}"#,
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
        r#"{"op":"set_launch_after_reboot","package":"com.example.app","enabled":"yes","client_build_id":"0.3.2"}"#,
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
        r#"{"op":"rebind_package","package":"com.example.app","signing":{"kind":"lineage","sha256":["bad"]},"trust_legacy":false,"client_build_id":"0.3.2"}"#,
        r#"{"op":"rebind_package","package":"com.example.app","signing":{"kind":"lineage","sha256":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],"future":true},"trust_legacy":false,"client_build_id":"0.3.2"}"#,
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
        r#"{"op":"enroll","package":"com.example.app","reset":false,"client_build_id":"0.3.2"}"#,
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
    runtime.enroll(package.clone(), None).unwrap();
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
fn probe_without_a_client_id_exposes_the_compiled_executable_identity() {
    let mut runtime = Runtime::new(
        MemoryPackageStore::default(),
        MemorySlotStorage::default(),
        MemoryAndroidOps::default(),
    );

    let response = handle_line(&mut runtime, CLIENT_BUILD_ID, r#"{"op":"probe"}"#);

    assert!(response.contains("build_id"));
    let body: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert_eq!(body["ok"]["binary_build_id"], crate::BINARY_BUILD_ID);
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
        "0.3.2",
        r#"{"op":"begin_backup_io","package":"com.example.app","scope":{"kind":"account","slot":"base"},"client_build_id":"0.3.2"}"#,
    );

    assert!(response.contains("\"io_token\""), "response: {response}");
    assert!(response.contains("\"sources\""), "response: {response}");
}

#[test]
fn account_io_status_is_queryable_without_mutating_package_state() {
    let mut runtime = runtime_with_slot();

    let response = handle_line(
        &mut runtime,
        "0.3.2",
        r#"{"op":"list_account_io_status","client_build_id":"0.3.2"}"#,
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
    runtime.enroll(package.clone(), None).unwrap();
    runtime
        .create_slot(&package, DisplayName::new("Work").unwrap(), SeedMode::Blank)
        .unwrap();
    runtime
}

#[test]
fn shared_archive_enums_keep_their_wire_names() {
    use crate::model::{ArchiveAccountKind, ArchiveDomain, ArchiveScope};

    for (value, wire) in [
        (serde_json::to_string(&ArchiveScope::Account), "\"account\""),
        (
            serde_json::to_string(&ArchiveScope::AllAccounts),
            "\"all_accounts\"",
        ),
        (serde_json::to_string(&ArchiveAccountKind::Base), "\"base\""),
        (serde_json::to_string(&ArchiveAccountKind::Slot), "\"slot\""),
        (serde_json::to_string(&ArchiveDomain::Ce), "\"ce\""),
        (serde_json::to_string(&ArchiveDomain::De), "\"de\""),
    ] {
        assert_eq!(value.unwrap(), wire);
    }
}

#[test]
fn shared_restore_fixture_round_trips_through_the_merged_types() {
    let request: serde_json::Value =
        serde_json::from_str(&fixture("begin_restore_io.request.json".to_owned())).unwrap();
    let mappings: Vec<RestoreMapping> =
        serde_json::from_value(request["mappings"].clone()).unwrap();
    let archived_state: ArchivedState =
        serde_json::from_value(request["archived_state"].clone()).unwrap();

    assert_eq!(
        serde_json::to_value(&mappings).unwrap(),
        request["mappings"]
    );
    assert_eq!(
        serde_json::to_value(&archived_state).unwrap(),
        request["archived_state"]
    );
}

fn request_fixtures() -> Vec<(String, serde_json::Map<String, serde_json::Value>)> {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../protocol/fixtures");
    let mut fixtures = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| name.ends_with(".request.json") && name != "enroll_reset.request.json")
        .map(|name| {
            let value: serde_json::Value = serde_json::from_str(&fixture(name.clone())).unwrap();
            (name, value.as_object().unwrap().clone())
        })
        .collect::<Vec<_>>();
    fixtures.sort_by(|left, right| left.0.cmp(&right.0));
    fixtures
}

fn decode(object: &serde_json::Map<String, serde_json::Value>) -> Result<Command, DecodeError> {
    decode_command(
        &serde_json::Value::Object(object.clone()).to_string(),
        CLIENT_BUILD_ID,
    )
}

#[test]
fn every_shared_request_fixture_decodes_with_its_flat_client_build_id() {
    let fixtures = request_fixtures();
    assert_eq!(fixtures.len(), 19);
    for (name, object) in fixtures {
        assert!(decode(&object).is_ok(), "fixture: {name}");
    }
}

#[test]
fn every_op_rejects_a_field_it_does_not_consume() {
    for (name, mut object) in request_fixtures() {
        object.insert("future".to_owned(), serde_json::Value::Bool(true));
        assert_eq!(
            decode(&object),
            Err(DecodeError::InvalidRequest),
            "fixture: {name}"
        );
    }
}

#[test]
fn client_build_id_is_required_exactly_on_every_op_but_probe() {
    for (name, mut object) in request_fixtures() {
        let is_probe = object["op"] == "probe";
        if is_probe {
            object.insert("client_build_id".to_owned(), CLIENT_BUILD_ID.into());
            assert_eq!(
                decode(&object),
                Err(DecodeError::InvalidRequest),
                "fixture: {name}"
            );
            continue;
        }
        object.remove("client_build_id");
        assert_eq!(
            decode(&object),
            Err(DecodeError::MissingClientBuildId),
            "fixture: {name}"
        );
        object.insert("client_build_id".to_owned(), serde_json::Value::from(23));
        assert_eq!(
            decode(&object),
            Err(DecodeError::MissingClientBuildId),
            "fixture: {name}"
        );
        object.insert("client_build_id".to_owned(), "0.0.1".into());
        assert_eq!(
            decode(&object),
            Err(DecodeError::ClientBuildMismatch),
            "fixture: {name}"
        );
    }
}

#[test]
fn malformed_frames_and_unknown_ops_are_invalid_requests() {
    for line in [
        "",
        "[]",
        r#""probe""#,
        r#"{"client_build_id":"0.3.2"}"#,
        r#"{"op":7,"client_build_id":"0.3.2"}"#,
        r#"{"op":"format_device","client_build_id":"0.3.2"}"#,
        r#"{"op":"rebind_package","package":"com.example.app","signing":{"kind":"lineage","sha256":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]},"trust_legacy":null,"client_build_id":"0.3.2"}"#,
        r#"{"op":"rebind_package","package":"com.example.app","signing":{"kind":"lineage","sha256":["aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]},"trust_legacy":"true","client_build_id":"0.3.2"}"#,
    ] {
        assert_eq!(
            decode_command(line, CLIENT_BUILD_ID),
            Err(DecodeError::InvalidRequest),
            "request: {line}"
        );
    }
}

#[test]
fn default_account_setting_changes_policy_without_switching_or_launching() {
    let mut runtime = runtime_with_slot();
    let request = format!(
        r#"{{"op":"set_default_slot","package":"com.example.app","slot":"slot-1","client_build_id":"{CLIENT_BUILD_ID}"}}"#,
    );
    let response: serde_json::Value =
        serde_json::from_str(&handle_line(&mut runtime, CLIENT_BUILD_ID, &request)).unwrap();
    assert_eq!(response["ok"]["package"]["default_slot"], "slot-1");
    assert_eq!(response["ok"]["package"]["active_slot"], "base");
}

#[test]
fn default_account_can_be_disabled_and_rejects_a_missing_slot() {
    let mut runtime = runtime_with_slot();
    for slot in [r#""base""#, "null"] {
        let request = format!(
            r#"{{"op":"set_default_slot","package":"com.example.app","slot":{slot},"client_build_id":"{CLIENT_BUILD_ID}"}}"#,
        );
        let response: serde_json::Value =
            serde_json::from_str(&handle_line(&mut runtime, CLIENT_BUILD_ID, &request)).unwrap();
        assert!(response.get("ok").is_some(), "{response}");
    }
    let request = format!(
        r#"{{"op":"set_default_slot","package":"com.example.app","slot":"slot-999","client_build_id":"{CLIENT_BUILD_ID}"}}"#,
    );
    let response: serde_json::Value =
        serde_json::from_str(&handle_line(&mut runtime, CLIENT_BUILD_ID, &request)).unwrap();
    assert_eq!(response["error"]["code"], "not_found");
}

#[test]
fn default_account_shared_fixture_is_a_complete_runtime_flow() {
    let mut runtime = runtime_with_slot();
    let actual = handle_line(
        &mut runtime,
        CLIENT_BUILD_ID,
        fixture("set_default_slot.request.json".to_owned()).trim(),
    );
    assert_eq!(
        actual.trim(),
        fixture("set_default_slot.response.json".to_owned()).trim()
    );
}

#[test]
fn default_account_setting_requires_explicit_slot_or_null() {
    let mut runtime = runtime_with_slot();
    for extra in ["", r#","slot":42"#, r#","slot":"../app""#] {
        let request = format!(
            r#"{{"op":"set_default_slot","package":"com.example.app"{extra},"client_build_id":"{CLIENT_BUILD_ID}"}}"#,
        );
        let response: serde_json::Value =
            serde_json::from_str(&handle_line(&mut runtime, CLIENT_BUILD_ID, &request)).unwrap();
        assert_eq!(response["error"]["code"], "invalid_request");
    }
}
