#![allow(missing_docs, clippy::unwrap_used)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;
use uclone_slot_runtime::compatibility_policy::CompatibilityPolicyStore;
use uclone_slot_runtime::domain::{AppIdentity, PackageName, PackageSupportLevel};

fn identity() -> AppIdentity {
    AppIdentity::new(
        10_332,
        &"f3".repeat(32),
        9_362_803,
        "/data/app/xhs/base.apk",
    )
    .unwrap()
}

fn package() -> PackageName {
    PackageName::parse("com.xingin.xhs").unwrap()
}

fn store() -> (tempfile::TempDir, CompatibilityPolicyStore) {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let store = CompatibilityPolicyStore::new(root.path().join("policy")).unwrap();
    (root, store)
}

#[test]
fn conditional_acceptance_is_identity_bound_and_digest_protected() {
    let (root, store) = store();
    store
        .create(
            &package(),
            &identity(),
            PackageSupportLevel::DirectBootConditional,
            true,
        )
        .unwrap();

    let policy = store.load(&package()).unwrap().unwrap();

    assert!(policy.accepts(&identity(), PackageSupportLevel::DirectBootConditional));
    let changed = AppIdentity::new(
        10_333,
        &"f3".repeat(32),
        9_362_803,
        "/data/app/xhs/base.apk",
    )
    .unwrap();
    assert!(!policy.accepts(&changed, PackageSupportLevel::DirectBootConditional));

    let path = root
        .path()
        .join("policy/packages/com.xingin.xhs/policy.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value.as_object_mut().unwrap().insert(
        "direct_boot_accepted".to_owned(),
        serde_json::Value::Bool(false),
    );
    fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(store.load(&package()).is_err());
}

#[test]
fn legacy_missing_policy_is_distinct_from_a_corrupt_policy() {
    let (_root, store) = store();
    assert_eq!(store.load(&package()).unwrap(), None);
}

#[test]
fn conditional_policy_cannot_be_published_without_exact_confirmation() {
    let (_root, store) = store();

    assert!(
        store
            .create(
                &package(),
                &identity(),
                PackageSupportLevel::DirectBootConditional,
                false,
            )
            .is_err()
    );
    assert!(
        store
            .create(
                &package(),
                &identity(),
                PackageSupportLevel::Supported,
                true,
            )
            .is_err()
    );
    assert_eq!(store.load(&package()).unwrap(), None);
}
