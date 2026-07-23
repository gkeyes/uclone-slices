#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test"
)]

use std::os::unix::fs::PermissionsExt as _;

use tempfile::TempDir;

use super::*;

const V1_NORMAL: &[u8] = include_bytes!("../../../tests/fixtures/package_state/v1/normal.json");
const V1_UPDATE_PREPARING: &[u8] =
    include_bytes!("../../../tests/fixtures/package_state/v1/update-preparing.json");
const V1_UPDATE_WINDOW_OPEN: &[u8] =
    include_bytes!("../../../tests/fixtures/package_state/v1/update-window-open.json");
const V1_UPDATE_VERIFYING: &[u8] =
    include_bytes!("../../../tests/fixtures/package_state/v1/update-verifying.json");
const V2_MIGRATED_NORMAL: &[u8] =
    include_bytes!("../../../tests/fixtures/package_state/v2/migrated-normal.json");
const V1_NORMAL_DIGEST: &str = "88a00cc35b49e4bd28e70033496ffe7370f9ce08e10e72785beec6d0809a2888";

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

fn artifact() -> InstalledArtifact {
    InstalledArtifact::new(42, "/data/app/~~fixture/com.uclone.fixture/base.apk").unwrap()
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
fn private_migration_is_append_only_and_byte_exact() {
    let (root, state, package) = raw_v1_store();
    let original_path = state.revision_path(package.package_name(), 1);
    let original = fs::read(&original_path).unwrap();
    let accepted = artifact();

    let migrated = state
        .migrate_v1_normal_to_v2(&package, 1, V1_NORMAL_DIGEST, accepted.clone())
        .unwrap();

    assert_eq!(fs::read(original_path).unwrap(), original);
    assert_eq!(migrated.schema_version(), 2);
    assert_eq!(migrated.generation(), 2);
    assert_eq!(migrated.previous_sha256(), Some(V1_NORMAL_DIGEST));
    assert_eq!(migrated.accepted_artifact(), Some(&accepted));
    assert_eq!(migrated.managed_update_context(), None);
    assert_eq!(
        fs::read(state.revision_path(package.package_name(), 2)).unwrap(),
        fixture(V2_MIGRATED_NORMAL)
    );

    let reopened = PackageStateStore::new(root.path()).unwrap();
    assert_eq!(reopened.latest(&package).unwrap(), Some(migrated));

    let recovery = reopened
        .transition(
            &package,
            LifecycleState::Normal,
            LifecycleState::RecoveryRequired,
            PackageStateReason::ViewUncertain,
        )
        .unwrap();
    assert_eq!(recovery.accepted_artifact(), Some(&accepted));
    assert_eq!(recovery.managed_update_context(), None);
}

#[test]
fn private_migration_requires_exact_head_and_runs_only_once() {
    let (_root, state) = store();
    let package = key("com.uclone.private.cas");
    let head = state.initialize(&package).unwrap();
    let accepted = artifact();

    for (generation, digest) in [
        (head.generation() + 1, head.sha256().to_owned()),
        (head.generation(), "0".repeat(64)),
    ] {
        let error = state
            .migrate_v1_normal_to_v2(&package, generation, &digest, accepted.clone())
            .unwrap_err();
        assert!(matches!(error, PackageStateError::UnexpectedHead { .. }));
    }
    assert!(
        !state
            .revision_path(package.package_name(), head.generation() + 1)
            .exists()
    );

    let migrated = state
        .migrate_v1_normal_to_v2(&package, head.generation(), head.sha256(), accepted.clone())
        .unwrap();
    let repeated = state
        .migrate_v1_normal_to_v2(&package, migrated.generation(), migrated.sha256(), accepted)
        .unwrap_err();
    assert!(matches!(
        repeated,
        PackageStateError::MigrationRefused {
            schema_version: 2,
            state: LifecycleState::Normal
        }
    ));
}

#[test]
fn private_migration_refuses_every_v1_non_normal_head() {
    let legacy_update_heads: &[(&[u8], LifecycleState, u64)] = &[
        (V1_UPDATE_PREPARING, LifecycleState::UpdatePreparing, 2),
        (V1_UPDATE_WINDOW_OPEN, LifecycleState::UpdateWindowOpen, 3),
        (V1_UPDATE_VERIFYING, LifecycleState::UpdateVerifying, 4),
    ];
    for &(fixture_bytes, expected_state, generation) in legacy_update_heads {
        assert_v1_update_head_refuses_migration(fixture_bytes, expected_state, generation);
    }
    let cases: &[(&str, &[(LifecycleState, PackageStateReason)])] = &[
        (
            "drifted",
            &[(
                LifecycleState::LifecycleDrifted,
                PackageStateReason::LifecycleDrift,
            )],
        ),
        (
            "repair",
            &[
                (
                    LifecycleState::LifecycleDrifted,
                    PackageStateReason::LifecycleDrift,
                ),
                (
                    LifecycleState::RepairWaiting,
                    PackageStateReason::ManualRepair,
                ),
            ],
        ),
        (
            "recovery",
            &[(
                LifecycleState::RecoveryRequired,
                PackageStateReason::ViewUncertain,
            )],
        ),
        (
            "quarantine",
            &[(
                LifecycleState::Quarantined,
                PackageStateReason::IdentityChanged,
            )],
        ),
    ];
    for &(name, transitions) in cases {
        let (_root, state) = store();
        let package = key(&format!("com.uclone.legacy.{name}"));
        let mut head = state.initialize(&package).unwrap();
        for &(next, reason) in transitions {
            head = state
                .transition(&package, head.lifecycle_state(), next, reason)
                .unwrap();
        }
        let error = state
            .migrate_v1_normal_to_v2(&package, head.generation(), head.sha256(), artifact())
            .unwrap_err();
        assert!(matches!(
            error,
            PackageStateError::MigrationRefused {
                schema_version: 1,
                state
            } if state == head.lifecycle_state()
        ));
        assert!(
            !state
                .revision_path(package.package_name(), head.generation() + 1)
                .exists()
        );
    }
}
fn assert_v1_update_head_refuses_migration(
    fixture_bytes: &'static [u8],
    expected_state: LifecycleState,
    generation: u64,
) {
    let (_root, state, package) = raw_v1_store();
    let fixtures = [
        V1_UPDATE_PREPARING,
        V1_UPDATE_WINDOW_OPEN,
        V1_UPDATE_VERIFYING,
    ];
    let package_name = package.package_name();
    let preceding = usize::try_from(generation - 1).unwrap();
    for (index, bytes) in fixtures.iter().enumerate().take(preceding) {
        let revision_generation = u64::try_from(index).unwrap() + 2;
        write_raw_revision(&state, package_name, revision_generation, fixture(bytes));
    }
    assert_eq!(
        fs::read(state.revision_path(package_name, generation)).unwrap(),
        fixture(fixture_bytes)
    );
    let head = state.latest(&package).unwrap().unwrap();
    assert_eq!(head.lifecycle_state(), expected_state);
    let error = state
        .migrate_v1_normal_to_v2(&package, head.generation(), head.sha256(), artifact())
        .unwrap_err();
    assert!(matches!(
        error,
        PackageStateError::MigrationRefused {
            schema_version: 1,
            state
        } if state == expected_state
    ));
    assert!(
        !state
            .revision_path(package.package_name(), head.generation() + 1)
            .exists()
    );
}
