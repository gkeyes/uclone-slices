#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test"
)]

use crate::domain::{
    GateSnapshot, InstalledArtifact, ManagedUpdateContext, PackageEnabledState, UpdateToken,
};

use super::*;

const NORMAL: &[u8] =
    include_bytes!("../../../../tests/fixtures/package_state/v2/schema/01-normal.json");
const PREPARING: &[u8] =
    include_bytes!("../../../../tests/fixtures/package_state/v2/schema/02-update-preparing.json");
const WINDOW: &[u8] =
    include_bytes!("../../../../tests/fixtures/package_state/v2/schema/03-update-window-open.json");
const VERIFYING: &[u8] =
    include_bytes!("../../../../tests/fixtures/package_state/v2/schema/04-update-verifying.json");
const PROMOTED: &[u8] =
    include_bytes!("../../../../tests/fixtures/package_state/v2/schema/05-normal-promoted.json");
const RECOVERY: &[u8] =
    include_bytes!("../../../../tests/fixtures/package_state/v2/schema/06-recovery-retained.json");
const V1_NORMAL: &[u8] = include_bytes!("../../../../tests/fixtures/package_state/v1/normal.json");

fn decode(bytes: &[u8]) -> PackageStateRevision {
    serde_json::from_slice(bytes).unwrap()
}

fn resign(revision: &mut PackageStateRevision) {
    revision.sha256 = revision.digest().unwrap();
}

#[test]
fn schema_v2_fixtures_decode_and_verify_without_a_store_writer() {
    let mut previous = None;
    for fixture in [NORMAL, PREPARING, WINDOW, VERIFYING, PROMOTED, RECOVERY] {
        let revision = decode(fixture);
        verify(&revision, previous.as_ref()).unwrap();
        previous = Some(revision);
    }

    let latest = previous.unwrap();
    assert_eq!(latest.lifecycle_state(), LifecycleState::RecoveryRequired);
    assert_eq!(latest.accepted_artifact().unwrap().version_code(), 43);
    assert_eq!(latest.managed_update_context(), None);

    let verifying = decode(VERIFYING);
    let context = verifying.managed_update_context().unwrap();
    assert_eq!(context.token().as_str(), "update-token-0001");
    assert_eq!(
        context.gate_snapshot(),
        GateSnapshot::new(PackageEnabledState::DisabledUser, true)
    );
    assert_eq!(context.candidate_artifact().unwrap().version_code(), 43);
}

#[test]
fn schema_v2_state_shapes_and_exact_context_are_enforced() {
    let normal = decode(NORMAL);
    let preparing = decode(PREPARING);
    let window = decode(WINDOW);

    let mut missing_context = preparing.clone();
    missing_context.managed_update_context = None;
    resign(&mut missing_context);
    assert!(verify(&missing_context, Some(&normal)).is_err());

    let mut missing_candidate = decode(VERIFYING);
    let context = missing_candidate.managed_update_context().unwrap();
    let token = context.token().clone();
    let gate = context.gate_snapshot();
    missing_candidate.managed_update_context = Some(ManagedUpdateContext::pending(token, gate));
    resign(&mut missing_candidate);
    assert!(verify(&missing_candidate, Some(&window)).is_err());

    let original = window.managed_update_context().unwrap();
    let original_token = original.token().clone();
    let original_gate = original.gate_snapshot();
    let mut changed_token = window.clone();
    changed_token.managed_update_context = Some(ManagedUpdateContext::pending(
        UpdateToken::parse("update-token-0002").unwrap(),
        original_gate,
    ));
    resign(&mut changed_token);
    assert!(verify(&changed_token, Some(&preparing)).is_err());

    let mut changed_gate = window;
    changed_gate.managed_update_context = Some(ManagedUpdateContext::pending(
        original_token,
        GateSnapshot::new(PackageEnabledState::Enabled, false),
    ));
    resign(&mut changed_gate);
    assert!(verify(&changed_gate, Some(&preparing)).is_err());
}

#[test]
fn schema_v2_terminal_states_retain_the_last_accepted_artifact() {
    let promoted = decode(PROMOTED);
    let mut changed = decode(RECOVERY);
    changed.accepted_artifact =
        Some(InstalledArtifact::new(42, "/data/app/~~fixture/com.uclone.schema/base.apk").unwrap());
    resign(&mut changed);

    assert!(verify(&changed, Some(&promoted)).is_err());
}

#[test]
fn generic_transition_cannot_cross_any_schema_update_state() {
    let legacy_normal = decode(V1_NORMAL);
    let normal = decode(NORMAL);
    let preparing = decode(PREPARING);
    let verifying = decode(VERIFYING);

    for (previous, next, reason) in [
        (
            &legacy_normal,
            LifecycleState::UpdatePreparing,
            PackageStateReason::ManagedUpdate,
        ),
        (
            &normal,
            LifecycleState::UpdatePreparing,
            PackageStateReason::ManagedUpdate,
        ),
        (
            &preparing,
            LifecycleState::RecoveryRequired,
            PackageStateReason::ViewUncertain,
        ),
        (
            &verifying,
            LifecycleState::Normal,
            PackageStateReason::ManagedUpdate,
        ),
    ] {
        let error = PackageStateRevision::transitioned(previous, next, reason).unwrap_err();
        assert!(error.to_string().contains("proof-bearing coordinator"));
    }

    let recovery = PackageStateRevision::transitioned(
        &normal,
        LifecycleState::RecoveryRequired,
        PackageStateReason::ViewUncertain,
    )
    .unwrap();
    assert_eq!(recovery.accepted_artifact(), normal.accepted_artifact());
    assert_eq!(recovery.managed_update_context(), None);
}
