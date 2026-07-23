use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use serde::Serialize;

use crate::domain::{PackageKey, PackageName, UserId};
use crate::lifecycle::LifecycleState;
use crate::package_state::{PackageStateReason, PackageStateStore};

#[derive(Serialize)]
struct UnsignedRevision<'a> {
    schema_version: u32,
    generation: u64,
    package_name: &'a PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    previous_sha256: &'a str,
}

#[derive(Serialize)]
struct Revision<'a> {
    schema_version: u32,
    generation: u64,
    package_name: &'a PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    previous_sha256: &'a str,
    sha256: &'a str,
}

pub(super) fn advance_to(store: &PackageStateStore, key: &PackageKey, target: LifecycleState) {
    let head = store.latest(key).unwrap().unwrap();
    assert_eq!(head.schema_version(), 1);
    assert_eq!(head.lifecycle_state(), LifecycleState::Normal);

    let stages = [
        LifecycleState::UpdatePreparing,
        LifecycleState::UpdateWindowOpen,
        LifecycleState::UpdateVerifying,
    ];
    let count = stages
        .iter()
        .position(|state| *state == target)
        .map(|index| index + 1)
        .unwrap();
    let mut generation = head.generation();
    let mut previous_sha256 = head.sha256().to_owned();

    for lifecycle_state in stages.into_iter().take(count) {
        generation += 1;
        let unsigned = UnsignedRevision {
            schema_version: 1,
            generation,
            package_name: key.package_name(),
            user_id: key.user_id(),
            lifecycle_state,
            reason: PackageStateReason::ManagedUpdate,
            previous_sha256: &previous_sha256,
        };
        let sha256 = crate::integrity::digest_json(&unsigned).unwrap();
        let revision = Revision {
            schema_version: 1,
            generation,
            package_name: key.package_name(),
            user_id: key.user_id(),
            lifecycle_state,
            reason: PackageStateReason::ManagedUpdate,
            previous_sha256: &previous_sha256,
            sha256: &sha256,
        };
        let path = store.revision_path(key.package_name(), generation);
        fs::write(&path, serde_json::to_vec(&revision).unwrap()).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        previous_sha256 = sha256;
    }

    let loaded = store.latest(key).unwrap().unwrap();
    assert_eq!(loaded.lifecycle_state(), target);
}
