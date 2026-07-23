#![doc = "Schema-v1 compatibility and schema-v2 package-state boundary tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{
    InstalledArtifact, ManagedUpdateContext, PackageKey, PackageName, UpdateToken, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::package_state::{PackageStateReason, PackageStateStore};

const V1_NORMAL: &[u8] = include_bytes!("fixtures/package_state/v1/normal.json");
const V1_UPDATE_PREPARING: &[u8] =
    include_bytes!("fixtures/package_state/v1/update-preparing.json");
const V2_MIGRATED_NORMAL: &[u8] = include_bytes!("fixtures/package_state/v2/migrated-normal.json");
const V1_NORMAL_DIGEST: &str = "88a00cc35b49e4bd28e70033496ffe7370f9ce08e10e72785beec6d0809a2888";
const V1_UPDATE_DIGEST: &str = "da86d8bdcb7e631547f4c85a20f5c9e53f18d508501235048b9d9d87b7e95eef";

fn fixture(bytes: &'static [u8]) -> &'static [u8] {
    bytes.strip_suffix(b"\n").unwrap_or(bytes)
}

fn key(name: &str) -> PackageKey {
    PackageKey::new(PackageName::parse(name).unwrap(), UserId::PRIMARY)
}

fn store() -> (TempDir, PackageStateStore) {
    let root = TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let state = PackageStateStore::new(root.path()).unwrap();
    (root, state)
}

fn write_raw_revision(
    state: &PackageStateStore,
    package: &PackageName,
    generation: u64,
    bytes: &[u8],
) {
    let package_directory = state.root().join("packages").join(package.as_str());
    let revisions = package_directory.join("revisions");
    fs::create_dir_all(&revisions).unwrap();
    fs::set_permissions(&package_directory, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&revisions, fs::Permissions::from_mode(0o700)).unwrap();
    let path = state.revision_path(package, generation);
    fs::write(&path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}

fn raw_v1_store() -> (TempDir, PackageStateStore, PackageKey) {
    let (root, state) = store();
    let package = key("com.uclone.fixture");
    write_raw_revision(&state, package.package_name(), 1, fixture(V1_NORMAL));
    (root, state, package)
}

#[test]
fn flat_schema_v1_initial_json_and_digest_remain_byte_exact() {
    let (_root, state) = store();
    let package = key("com.uclone.fixture");

    let first = state.initialize(&package).unwrap();
    assert_eq!(first.schema_version(), 1);
    assert_eq!(first.sha256(), V1_NORMAL_DIGEST);
    assert_eq!(first.accepted_artifact(), None);
    assert_eq!(
        fs::read(state.revision_path(package.package_name(), 1)).unwrap(),
        fixture(V1_NORMAL)
    );
}

#[test]
fn raw_v1_transitional_fixture_remains_read_compatible() {
    let (_root, state, package) = raw_v1_store();
    write_raw_revision(
        &state,
        package.package_name(),
        2,
        fixture(V1_UPDATE_PREPARING),
    );

    let head = state.latest(&package).unwrap().unwrap();
    assert_eq!(head.schema_version(), 1);
    assert_eq!(head.lifecycle_state(), LifecycleState::UpdatePreparing);
    assert_eq!(head.sha256(), V1_UPDATE_DIGEST);
    assert_eq!(head.accepted_artifact(), None);
    assert_eq!(head.managed_update_context(), None);
}

#[test]
fn generic_transition_cannot_enter_or_continue_legacy_update_states() {
    let (_root, state) = store();
    let package = key("com.uclone.generic.updateentry");
    state.initialize(&package).unwrap();
    assert!(
        state
            .transition(
                &package,
                LifecycleState::Normal,
                LifecycleState::UpdatePreparing,
                PackageStateReason::ManagedUpdate,
            )
            .is_err()
    );
    assert_eq!(state.latest(&package).unwrap().unwrap().generation(), 1);

    let (_root, state, package) = raw_v1_store();
    write_raw_revision(
        &state,
        package.package_name(),
        2,
        fixture(V1_UPDATE_PREPARING),
    );
    assert!(
        state
            .transition(
                &package,
                LifecycleState::UpdatePreparing,
                LifecycleState::UpdateWindowOpen,
                PackageStateReason::ManagedUpdate,
            )
            .is_err()
    );
    assert_eq!(state.latest(&package).unwrap().unwrap().generation(), 2);
}

#[test]
fn corrupt_v1_v2_future_and_v2_fields_under_v1_fail_closed() {
    let (_root, state, package) = raw_v1_store();
    let path = state.revision_path(package.package_name(), 1);
    let corrupt_v1 = String::from_utf8(fixture(V1_NORMAL).to_vec())
        .unwrap()
        .replace(
            V1_NORMAL_DIGEST,
            "08a00cc35b49e4bd28e70033496ffe7370f9ce08e10e72785beec6d0809a2888",
        );
    fs::write(&path, corrupt_v1).unwrap();
    assert!(state.latest(&package).is_err());

    let (_root, state, package) = raw_v1_store();
    let future = String::from_utf8(fixture(V1_NORMAL).to_vec())
        .unwrap()
        .replace("\"schema_version\":1", "\"schema_version\":3");
    write_raw_revision(&state, package.package_name(), 1, future.as_bytes());
    assert!(state.latest(&package).is_err());

    let (_root, state, package) = raw_v1_store();
    let mut forged = serde_json::from_slice::<serde_json::Value>(fixture(V1_NORMAL)).unwrap();
    let fields = forged.as_object_mut().unwrap();
    fields.insert(
        "accepted_artifact".to_owned(),
        serde_json::json!({
            "version_code": 42,
            "code_path": "/data/app/~~fixture/com.uclone.fixture/base.apk"
        }),
    );
    fields.insert("managed_update_context".to_owned(), serde_json::Value::Null);
    write_raw_revision(
        &state,
        package.package_name(),
        1,
        &serde_json::to_vec(&forged).unwrap(),
    );
    assert!(state.latest(&package).is_err());

    let (_root, state, package) = raw_v1_store();
    let corrupt_v2 = String::from_utf8(fixture(V2_MIGRATED_NORMAL).to_vec())
        .unwrap()
        .replace("\"version_code\":42", "\"version_code\":43");
    write_raw_revision(&state, package.package_name(), 2, corrupt_v2.as_bytes());
    assert!(state.latest(&package).is_err());

    let (_root, state, package) = raw_v1_store();
    let mut missing_artifact =
        serde_json::from_slice::<serde_json::Value>(fixture(V2_MIGRATED_NORMAL)).unwrap();
    missing_artifact
        .as_object_mut()
        .unwrap()
        .remove("accepted_artifact");
    write_raw_revision(
        &state,
        package.package_name(),
        2,
        &serde_json::to_vec(&missing_artifact).unwrap(),
    );
    assert!(state.latest(&package).is_err());
}

#[test]
fn owned_artifact_update_token_and_nested_gate_are_strictly_validated() {
    assert!(InstalledArtifact::new(0, "/data/app/example/base.apk").is_err());
    assert!(InstalledArtifact::new(1, "/system/app/example/base.apk").is_err());
    assert!(UpdateToken::parse("short").is_err());
    assert!(UpdateToken::parse("update token with spaces").is_err());

    let context_with_forged_gate_field = serde_json::json!({
        "token": "update-token-0001",
        "gate_snapshot": {
            "enabled_state": "enabled",
            "suspended": false,
            "forged": true
        },
        "candidate_artifact": null
    });
    assert!(
        serde_json::from_value::<ManagedUpdateContext>(context_with_forged_gate_field).is_err()
    );
}
