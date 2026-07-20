#![doc = "Strict bridge response schema tests."]

use serde_json::json;
use uclone_slot_runtime::bridge::{
    BridgeErrorCode, BridgePayload, PackageEnabledState, decode_response,
};

fn package_payload() -> serde_json::Value {
    json!({
        "type": "package",
        "packageName": "com.uclone.slotprobe",
        "userId": 0,
        "uid": 12345,
        "signatureSha256": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "versionCode": 7,
        "versionName": "0.1-preview",
        "codePath": "/data/app/com.uclone.slotprobe/base.apk",
        "ceDataPath": "/data/user/0/com.uclone.slotprobe",
        "deDataPath": "/data/user_de/0/com.uclone.slotprobe",
        "packageManagerCeInode": 101,
        "packageManagerDeInode": 202,
        "enabledState": "enabled",
        "suspended": false,
        "pendingInstall": false
    })
}

fn package_response() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "requestId": "package",
        "ok": true,
        "payload": package_payload()
    }))
    .unwrap_or_default()
}

fn set_field(value: &mut serde_json::Value, key: &str, replacement: serde_json::Value) {
    let updated = value.as_object_mut().map(|object| {
        object.insert(key.to_owned(), replacement);
    });
    assert!(updated.is_some());
}

#[test]
fn valid_package_response_is_typed_and_normalized() {
    let response = decode_response(&package_response()).expect("valid package response");
    let Some(BridgePayload::Package(package)) = response.payload() else {
        panic!("expected package payload");
    };
    assert_eq!(package.package_name(), "com.uclone.slotprobe");
    assert_eq!(package.user_id(), 0);
    assert_eq!(package.uid(), 12345);
    let expected_digest = "a".repeat(64);
    assert_eq!(package.signature_sha256(), expected_digest);
    assert_eq!(package.version_code(), 7);
    assert_eq!(package.version_name(), Some("0.1-preview"));
    assert_eq!(
        package.code_path(),
        "/data/app/com.uclone.slotprobe/base.apk"
    );
    assert_eq!(package.ce_data_path(), "/data/user/0/com.uclone.slotprobe");
    assert_eq!(
        package.de_data_path(),
        "/data/user_de/0/com.uclone.slotprobe"
    );
    assert_eq!(package.package_manager_ce_inode(), 101);
    assert_eq!(package.package_manager_de_inode(), 202);
    assert_eq!(package.enabled_state(), PackageEnabledState::Enabled);
    assert!(!package.suspended());
    assert!(!package.pending_install());
}

#[test]
fn device_response_requires_user_and_unlock_fields() {
    let bytes = serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "requestId": "device",
        "ok": true,
        "payload": {"type": "device", "userId": 0, "unlocked": true}
    }))
    .unwrap_or_default();
    let response = decode_response(&bytes).expect("valid device response");
    let Some(BridgePayload::Device(device)) = response.payload() else {
        panic!("expected device payload");
    };
    assert_eq!(device.user_id(), 0);
    assert!(device.unlocked());
}

#[test]
fn deny_unknown_fields_at_envelope_and_payload() {
    let mut root = json!({
        "schemaVersion": 1,
        "requestId": "package",
        "ok": true,
        "payload": package_payload()
    });
    set_field(&mut root, "unexpected", json!(true));
    let root_bytes = serde_json::to_vec(&root).unwrap_or_default();
    assert_eq!(
        decode_response(&root_bytes).unwrap_err().code(),
        BridgeErrorCode::InvalidResponse
    );

    let mut payload = package_payload();
    set_field(&mut payload, "unexpected", json!(true));
    let nested = json!({
        "schemaVersion": 1,
        "requestId": "package",
        "ok": true,
        "payload": payload
    });
    let nested_bytes = serde_json::to_vec(&nested).unwrap_or_default();
    assert_eq!(
        decode_response(&nested_bytes).unwrap_err().code(),
        BridgeErrorCode::InvalidResponse
    );
}

#[test]
fn malformed_digest_and_paths_are_rejected() {
    let mut digest = package_payload();
    set_field(&mut digest, "signatureSha256", json!("not-a-digest"));
    let digest_bytes = serde_json::to_vec(&json!({
        "schemaVersion": 1, "requestId": "package", "ok": true, "payload": digest
    }))
    .unwrap_or_default();
    assert_eq!(
        decode_response(&digest_bytes).unwrap_err().code(),
        BridgeErrorCode::InvalidResponse
    );

    let mut path = package_payload();
    set_field(&mut path, "codePath", json!("/data/app/../evil.apk"));
    let path_bytes = serde_json::to_vec(&json!({
        "schemaVersion": 1, "requestId": "package", "ok": true, "payload": path
    }))
    .unwrap_or_default();
    assert_eq!(
        decode_response(&path_bytes).unwrap_err().code(),
        BridgeErrorCode::InvalidResponse
    );

    let mut data_path = package_payload();
    set_field(
        &mut data_path,
        "ceDataPath",
        json!("/data/user/0/com.other.app"),
    );
    let data_bytes = serde_json::to_vec(&json!({
        "schemaVersion": 1, "requestId": "package", "ok": true, "payload": data_path
    }))
    .unwrap_or_default();
    assert_eq!(
        decode_response(&data_bytes).unwrap_err().code(),
        BridgeErrorCode::InvalidResponse
    );
}

#[test]
fn oversized_output_is_rejected_before_json_parsing() {
    let bytes = vec![b' '; 16 * 1024 + 1];
    assert_eq!(
        decode_response(&bytes).unwrap_err().code(),
        BridgeErrorCode::ResponseTooLarge
    );
}

#[test]
fn response_shape_and_request_id_are_strict() {
    let bytes = serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "requestId": "package",
        "ok": true,
        "errorCode": "internal"
    }))
    .unwrap_or_default();
    assert_eq!(
        decode_response(&bytes).unwrap_err().code(),
        BridgeErrorCode::InvalidResponse
    );

    let bytes = serde_json::to_vec(&json!({
        "schemaVersion": 1,
        "requestId": "package;id",
        "ok": false,
        "errorCode": "internal"
    }))
    .unwrap_or_default();
    assert_eq!(
        decode_response(&bytes).unwrap_err().code(),
        BridgeErrorCode::InvalidResponse
    );
}
