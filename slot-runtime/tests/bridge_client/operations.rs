use super::*;

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
            .set_enabled(ALLOWED_PACKAGE, 0, PackageEnabledState::DisabledUser)
            .is_ok()
    );
    assert_eq!(
        client.into_inner().seen,
        vec![BridgeCommand::SetEnabled(
            package_name(ALLOWED_PACKAGE),
            PackageEnabledState::DisabledUser
        )]
    );
    let identity = expected_identity();
    let base = expected_base_inodes();
    let mut client = BridgeClient::new(FakeRunner::ok(ack_bytes("restore-enabled")));
    assert!(
        client
            .restore_enabled(
                ALLOWED_PACKAGE,
                0,
                PackageEnabledState::Default,
                &identity,
                base,
            )
            .is_ok()
    );
    assert_eq!(
        client.into_inner().seen,
        vec![BridgeCommand::RestoreEnabled {
            package: package_name(ALLOWED_PACKAGE),
            state: PackageEnabledState::Default,
            expected_identity: identity.clone(),
            expected_base_inodes: base,
        }]
    );
    let mut client = BridgeClient::new(FakeRunner::ok(ack_bytes("restore-suspended")));
    assert!(
        client
            .restore_suspended(ALLOWED_PACKAGE, 0, true, &identity, base)
            .is_ok()
    );
    assert_eq!(
        client.into_inner().seen,
        vec![BridgeCommand::RestoreSuspended {
            package: package_name(ALLOWED_PACKAGE),
            suspended: true,
            expected_identity: identity.clone(),
            expected_base_inodes: base,
        }]
    );
    let mut client = BridgeClient::new(FakeRunner::ok(ack_bytes("launch-package")));
    assert!(
        client
            .launch_package(ALLOWED_PACKAGE, 0, &identity, base)
            .is_ok()
    );
    assert_eq!(
        client.into_inner().seen,
        vec![BridgeCommand::LaunchPackage {
            package: package_name(ALLOWED_PACKAGE),
            expected_identity: identity,
            expected_base_inodes: base,
        }]
    );
}

#[test]
fn unchecked_enabled_mutation_cannot_release_the_gate() {
    let mut client = BridgeClient::new(FakeRunner::ok(ack_bytes("set-enabled")));

    let error = client
        .set_enabled(ALLOWED_PACKAGE, 0, PackageEnabledState::Enabled)
        .unwrap_err();

    assert_eq!(error.code(), BridgeErrorCode::InvalidRequest);
    assert!(client.into_inner().seen.is_empty());
}

#[test]
fn missing_launcher_entry_is_preserved_as_a_typed_bridge_failure() {
    let bytes = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "buildId": uclone_slot_runtime::protocol::RUNTIME_BUILD_ID,
        "requestId": "launch-package",
        "ok": false,
        "errorCode": "launch_entry_not_found"
    }))
    .unwrap_or_default();
    let mut client = BridgeClient::new(FakeRunner::ok(bytes));

    let error = client
        .launch_package(
            ALLOWED_PACKAGE,
            0,
            &expected_identity(),
            expected_base_inodes(),
        )
        .unwrap_err();

    assert_eq!(error.code(), BridgeErrorCode::LaunchEntryNotFound);
}

#[test]
fn launch_identity_drift_is_preserved_as_a_typed_bridge_failure() {
    let bytes = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "buildId": uclone_slot_runtime::protocol::RUNTIME_BUILD_ID,
        "requestId": "launch-package",
        "ok": false,
        "errorCode": "identity_changed"
    }))
    .unwrap_or_default();
    let mut client = BridgeClient::new(FakeRunner::ok(bytes));

    let error = client
        .launch_package(
            ALLOWED_PACKAGE,
            0,
            &expected_identity(),
            expected_base_inodes(),
        )
        .unwrap_err();

    assert_eq!(error.code(), BridgeErrorCode::IdentityChanged);
}

#[test]
fn legacy_v1_read_is_compatible_but_mutation_is_fail_closed() {
    let mut read_client = BridgeClient::new(FakeRunner::ok(device_bytes(0)));
    assert!(read_client.query_device(0).is_ok());

    let mut mutation_client = BridgeClient::new(FakeRunner::ok(
        serde_json::to_vec(&json!({
            "schemaVersion": 1,
            "requestId": "set-enabled",
            "ok": true,
            "payload": {"type": "ack"}
        }))
        .unwrap_or_default(),
    ));
    let error = mutation_client
        .set_enabled(ALLOWED_PACKAGE, 0, PackageEnabledState::DisabledUser)
        .unwrap_err();

    assert_eq!(error.code(), BridgeErrorCode::BuildMismatch);
}

#[test]
fn paired_v2_build_mismatch_is_rejected_before_ack_is_accepted() {
    let bytes = serde_json::to_vec(&json!({
        "schemaVersion": 2,
        "buildId": "other-build",
        "requestId": "set-enabled",
        "ok": true,
        "payload": {"type": "ack"}
    }))
    .unwrap_or_default();
    let mut client = BridgeClient::new(FakeRunner::ok(bytes));

    let error = client
        .set_enabled(ALLOWED_PACKAGE, 0, PackageEnabledState::DisabledUser)
        .unwrap_err();

    assert_eq!(error.code(), BridgeErrorCode::BuildMismatch);
}
