#![doc = "Typed bridge client boundary tests."]
#![allow(
    clippy::unwrap_used,
    reason = "fixed valid package literals are test fixture invariants"
)]

use serde_json::json;
use uclone_slot_runtime::bridge::{
    ALLOWED_PACKAGE, BridgeClient, BridgeCommand, BridgeErrorCode, BridgeRunnerError,
    MAX_OUTPUT_BYTES, PackageEnabledState,
};
use uclone_slot_runtime::domain::PackageName;

fn package_name(raw: &str) -> PackageName {
    PackageName::parse(raw).unwrap()
}

#[derive(Debug)]
struct FakeRunner {
    result: Result<Vec<u8>, BridgeRunnerError>,
    seen: Vec<BridgeCommand>,
}

impl FakeRunner {
    const fn ok(bytes: Vec<u8>) -> Self {
        Self {
            result: Ok(bytes),
            seen: Vec::new(),
        }
    }

    const fn failure(error: BridgeRunnerError) -> Self {
        Self {
            result: Err(error),
            seen: Vec::new(),
        }
    }
}

impl uclone_slot_runtime::bridge::BridgeCommandRunner for FakeRunner {
    fn run(&mut self, command: &BridgeCommand) -> Result<Vec<u8>, BridgeRunnerError> {
        self.seen.push(command.clone());
        match &self.result {
            Ok(bytes) => Ok(bytes.clone()),
            Err(BridgeRunnerError::NonZeroExit { status }) => {
                Err(BridgeRunnerError::NonZeroExit { status: *status })
            }
            Err(BridgeRunnerError::OutputTooLarge { size }) => {
                Err(BridgeRunnerError::OutputTooLarge { size: *size })
            }
            Err(BridgeRunnerError::TimedOut) => Err(BridgeRunnerError::TimedOut),
            Err(BridgeRunnerError::Io(_)) => Err(BridgeRunnerError::Io(std::io::Error::other(
                "fake runner unavailable",
            ))),
        }
    }
}

fn device_bytes(user_id: u32) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 1, "requestId": "device", "ok": true,
        "payload": {"type": "device", "userId": user_id, "unlocked": true}
    }))
    .unwrap_or_default()
}

fn package_bytes(package: &str, user_id: u32) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 1, "requestId": "package", "ok": true,
        "payload": {
            "type": "package", "packageName": package, "userId": user_id,
            "uid": 12345, "signatureSha256": "aa".repeat(32), "versionCode": 1,
            "versionName": "0.1-preview", "codePath": format!("/data/app/{package}/base.apk"),
            "ceDataPath": format!("/data/user/0/{package}"),
            "deDataPath": format!("/data/user_de/0/{package}"),
            "packageManagerCeInode": 101,
            "packageManagerDeInode": 202,
            "enabledState": "enabled", "suspended": false, "pendingInstall": false,
            "systemApp": false, "sharedUid": false, "directBootAware": false
        }
    }))
    .unwrap_or_default()
}

fn ack_bytes(request_id: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 1, "requestId": request_id, "ok": true,
        "payload": {"type": "ack"}
    }))
    .unwrap_or_default()
}

fn gate_bytes() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "schemaVersion": 1, "requestId": "gate", "ok": true,
        "payload": {
            "type": "gate", "enabledState": "disabled_user", "suspended": true
        }
    }))
    .unwrap_or_default()
}

#[test]
fn lightweight_gate_query_uses_its_fixed_payload_without_package_metadata() {
    let mut client = BridgeClient::new(FakeRunner::ok(gate_bytes()));

    let gate = client.query_gate(ALLOWED_PACKAGE, 0).unwrap();

    assert_eq!(gate.enabled_state(), PackageEnabledState::DisabledUser);
    assert!(gate.suspended());
    assert_eq!(
        client.into_inner().seen,
        vec![BridgeCommand::GateStatus(package_name(ALLOWED_PACKAGE))]
    );
}

#[test]
fn mismatched_package_user_and_injection_are_rejected_before_runner() {
    for package in [
        "single",
        "com.uclone.slotprobe;id",
        "com.uclone.slotprobe $(id)",
    ] {
        let mut client = BridgeClient::new(FakeRunner::ok(package_bytes(ALLOWED_PACKAGE, 0)));
        assert_eq!(
            client.query_package(package, 0).unwrap_err().code(),
            BridgeErrorCode::PackageNotAllowed
        );
        assert!(client.into_inner().seen.is_empty());
    }
    let mut client = BridgeClient::new(FakeRunner::ok(package_bytes(ALLOWED_PACKAGE, 0)));
    assert_eq!(
        client
            .query_package(ALLOWED_PACKAGE, 10)
            .unwrap_err()
            .code(),
        BridgeErrorCode::UserNotAllowed
    );
    assert!(client.into_inner().seen.is_empty());
}

#[test]
fn response_package_and_user_mismatch_are_rejected() {
    let mut package_client = BridgeClient::new(FakeRunner::ok(package_bytes("com.other.app", 0)));
    assert_eq!(
        package_client
            .query_package(ALLOWED_PACKAGE, 0)
            .unwrap_err()
            .code(),
        BridgeErrorCode::RequestMismatch
    );
    let mut user_client = BridgeClient::new(FakeRunner::ok(package_bytes(ALLOWED_PACKAGE, 10)));
    assert_eq!(
        user_client
            .query_package(ALLOWED_PACKAGE, 0)
            .unwrap_err()
            .code(),
        BridgeErrorCode::UserNotAllowed
    );
    let mut device_client = BridgeClient::new(FakeRunner::ok(device_bytes(10)));
    assert_eq!(
        device_client.query_device(0).unwrap_err().code(),
        BridgeErrorCode::UserNotAllowed
    );
}

#[test]
fn runner_failures_map_to_stable_codes() {
    let mut nonzero = BridgeClient::new(FakeRunner::failure(BridgeRunnerError::NonZeroExit {
        status: Some(23),
    }));
    assert_eq!(
        nonzero.query_device(0).unwrap_err().code(),
        BridgeErrorCode::CommandFailed
    );
    let mut oversized = BridgeClient::new(FakeRunner::failure(BridgeRunnerError::OutputTooLarge {
        size: MAX_OUTPUT_BYTES + 1,
    }));
    assert_eq!(
        oversized.query_device(0).unwrap_err().code(),
        BridgeErrorCode::ResponseTooLarge
    );
    let mut unavailable = BridgeClient::new(FakeRunner::failure(BridgeRunnerError::Io(
        std::io::Error::other("not available"),
    )));
    assert_eq!(
        unavailable.query_device(0).unwrap_err().code(),
        BridgeErrorCode::RunnerUnavailable
    );
    let mut timed_out = BridgeClient::new(FakeRunner::failure(BridgeRunnerError::TimedOut));
    assert_eq!(
        timed_out.query_device(0).unwrap_err().code(),
        BridgeErrorCode::TimedOut
    );
}

#[test]
fn request_and_payload_types_are_verified() {
    let wrong_request = serde_json::to_vec(&json!({
        "schemaVersion": 1, "requestId": "package", "ok": true,
        "payload": {"type": "device", "userId": 0, "unlocked": true}
    }))
    .unwrap_or_default();
    let mut client = BridgeClient::new(FakeRunner::ok(wrong_request));
    assert_eq!(
        client.query_device(0).unwrap_err().code(),
        BridgeErrorCode::RequestMismatch
    );
    let wrong_payload = serde_json::to_vec(&json!({
        "schemaVersion": 1, "requestId": "device", "ok": true,
        "payload": {"type": "ack"}
    }))
    .unwrap_or_default();
    let mut client = BridgeClient::new(FakeRunner::ok(wrong_payload));
    assert_eq!(
        client.query_device(0).unwrap_err().code(),
        BridgeErrorCode::InvalidResponse
    );
}

#[test]
fn typed_mutations_require_ack_and_fixed_target() {
    let mut client = BridgeClient::new(FakeRunner::ok(ack_bytes("set-enabled")));
    assert!(
        client
            .set_enabled(ALLOWED_PACKAGE, 0, PackageEnabledState::Disabled)
            .is_ok()
    );
    assert_eq!(
        client.into_inner().seen,
        vec![BridgeCommand::SetEnabled(
            package_name(ALLOWED_PACKAGE),
            PackageEnabledState::Disabled
        )]
    );
    let mut client = BridgeClient::new(FakeRunner::ok(ack_bytes("set-suspended")));
    assert!(client.set_suspended(ALLOWED_PACKAGE, 0, true).is_ok());
    assert_eq!(
        client.into_inner().seen,
        vec![BridgeCommand::SetSuspended(
            package_name(ALLOWED_PACKAGE),
            true
        )]
    );
}
