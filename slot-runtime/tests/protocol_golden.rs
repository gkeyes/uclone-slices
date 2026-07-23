#![allow(
    missing_docs,
    clippy::unwrap_used,
    reason = "golden fixture failures should identify the exact wire record"
)]

use serde::Deserialize;
use serde_json::Value;
use uclone_slot_runtime::protocol::{
    RUNTIME_BUILD_ID, SCHEMA_VERSION, decode_request, decode_response, encode_request,
    encode_response,
};

const REQUEST_FIXTURES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../protocol-fixtures/v2/requests.jsonl"
));
const RESPONSE_FIXTURES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../protocol-fixtures/v2/responses.jsonl"
));
const ERROR_FIXTURES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../protocol-fixtures/v2/errors.jsonl"
));
const MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../protocol-fixtures/v2/manifest.json"
));

const COMMANDS: [&str; 17] = [
    "probe",
    "inspect_package",
    "list_managed_apps",
    "list_recovery_targets",
    "enroll_package",
    "status_package",
    "package_snapshot",
    "create_slot",
    "list_slots",
    "switch",
    "launch_current",
    "rename_slot",
    "delete_slot",
    "reconcile",
    "reconcile_package",
    "retire_package",
    "rescue_to_base",
];

const PAYLOAD_KINDS: [&str; 11] = [
    "probe_report",
    "package_inspection",
    "managed_apps",
    "recovery_targets",
    "slots",
    "package_status",
    "package_snapshot",
    "switch_result",
    "launch_result",
    "reconcile_report",
    "ack",
];

const ERROR_CODES: [&str; 13] = [
    "invalid_request",
    "unsupported_schema",
    "runtime_pair_mismatch",
    "package_not_allowed",
    "direct_boot_confirmation_required",
    "not_found",
    "conflict",
    "recovery_required",
    "busy",
    "quarantined",
    "user_locked",
    "unsupported_device",
    "internal",
];

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    build_id: String,
    requests: ManifestSection,
    responses: ManifestSection,
    errors: ManifestSection,
}

#[derive(Debug, Deserialize)]
struct ManifestSection {
    file: String,
    count: usize,
    #[serde(default)]
    commands: Vec<String>,
    #[serde(default)]
    payload_kinds: Vec<String>,
    #[serde(default)]
    error_codes: Vec<String>,
}

fn fixture_lines(source: &str) -> Vec<&str> {
    source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect()
}

fn frame(line: &str) -> Vec<u8> {
    format!("{line}\n").into_bytes()
}

fn string_field(value: &Value, field: &str) -> String {
    value.get(field).and_then(Value::as_str).unwrap().to_owned()
}

fn manifest() -> Manifest {
    serde_json::from_str(MANIFEST).unwrap()
}

#[test]
fn request_fixtures_cover_every_command_and_round_trip_exactly() {
    let manifest = manifest();
    let lines = fixture_lines(REQUEST_FIXTURES);
    assert_eq!(manifest.schema_version, SCHEMA_VERSION);
    assert_eq!(manifest.build_id, "development");
    assert_eq!(manifest.build_id, RUNTIME_BUILD_ID);
    assert_eq!(manifest.requests.file, "requests.jsonl");
    assert_eq!(manifest.requests.count, COMMANDS.len());
    assert_eq!(manifest.requests.commands, COMMANDS);
    assert_eq!(lines.len(), COMMANDS.len());

    let mut commands = Vec::with_capacity(lines.len());
    for line in lines {
        let value: Value = serde_json::from_str(line).unwrap();
        commands.push(string_field(&value, "command"));
        let input = frame(line);
        let request = decode_request(&input).unwrap();
        assert_eq!(
            encode_request(&request).unwrap(),
            input,
            "request fixture: {line}"
        );
    }
    assert_eq!(commands, COMMANDS);
}

#[test]
fn response_fixtures_cover_every_payload_kind_and_round_trip_exactly() {
    let manifest = manifest();
    let lines = fixture_lines(RESPONSE_FIXTURES);
    assert_eq!(manifest.responses.file, "responses.jsonl");
    assert_eq!(manifest.responses.count, PAYLOAD_KINDS.len());
    assert_eq!(manifest.responses.payload_kinds, PAYLOAD_KINDS);
    assert_eq!(lines.len(), PAYLOAD_KINDS.len());

    let mut kinds = Vec::with_capacity(lines.len());
    for line in lines {
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(value.get("status").and_then(Value::as_str), Some("ok"));
        let payload = value.get("payload").unwrap();
        kinds.push(string_field(payload, "kind"));
        let input = frame(line);
        let response = decode_response(&input).unwrap();
        assert_eq!(
            encode_response(&response).unwrap(),
            input,
            "response fixture: {line}"
        );
    }
    assert_eq!(kinds, PAYLOAD_KINDS);
}

#[test]
fn error_fixtures_cover_every_error_code_and_round_trip_exactly() {
    let manifest = manifest();
    let lines = fixture_lines(ERROR_FIXTURES);
    assert_eq!(manifest.errors.file, "errors.jsonl");
    assert_eq!(manifest.errors.count, ERROR_CODES.len());
    assert_eq!(manifest.errors.error_codes, ERROR_CODES);
    assert_eq!(lines.len(), ERROR_CODES.len());

    let mut codes = Vec::with_capacity(lines.len());
    for line in lines {
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(value.get("status").and_then(Value::as_str), Some("error"));
        codes.push(string_field(&value, "error_code"));
        let input = frame(line);
        let response = decode_response(&input).unwrap();
        assert_eq!(
            encode_response(&response).unwrap(),
            input,
            "error fixture: {line}"
        );
    }
    assert_eq!(codes, ERROR_CODES);
}
