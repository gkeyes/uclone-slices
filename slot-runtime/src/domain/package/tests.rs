#![allow(
    clippy::unwrap_used,
    reason = "validated package identity fixtures must abort the individual test on failure"
)]

use super::{AppIdentity, AppOwnerIdentityRef, InstalledArtifactRef};

const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_SIGNATURE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn identity(uid: u32, signature: &str, version_code: u64, code_path: &str) -> AppIdentity {
    AppIdentity::new(uid, signature, version_code, code_path).unwrap()
}

#[test]
fn owner_identity_is_independent_from_installed_artifact() {
    let base = identity(10_321, SIGNATURE, 1, "/data/app/example/base.apk");
    let version_drift = identity(10_321, SIGNATURE, 2, "/data/app/example/base.apk");
    let path_drift = identity(10_321, SIGNATURE, 1, "/data/app/example/updated.apk");

    assert_eq!(base.owner_identity(), version_drift.owner_identity());
    assert_eq!(base.owner_identity(), path_drift.owner_identity());
    assert_ne!(base, version_drift);
    assert_ne!(base, path_drift);
    assert_ne!(
        base.installed_artifact(),
        version_drift.installed_artifact()
    );
    assert_ne!(base.installed_artifact(), path_drift.installed_artifact());
}

#[test]
fn artifact_equality_detects_version_and_code_path_drift() {
    let base = identity(10_321, SIGNATURE, 7, "/data/app/example/base.apk");
    let same_artifact = identity(10_999, OTHER_SIGNATURE, 7, "/data/app/example/base.apk");
    let version_drift = identity(10_999, OTHER_SIGNATURE, 8, "/data/app/example/base.apk");
    let path_drift = identity(10_999, OTHER_SIGNATURE, 7, "/data/app/example/updated.apk");

    assert_eq!(
        base.installed_artifact(),
        same_artifact.installed_artifact()
    );
    assert_ne!(base, same_artifact);
    assert_ne!(
        base.installed_artifact(),
        version_drift.installed_artifact()
    );
    assert_ne!(base.installed_artifact(), path_drift.installed_artifact());
    assert_ne!(base.owner_identity(), same_artifact.owner_identity());
    let owner: AppOwnerIdentityRef<'_> = base.owner_identity();
    let artifact: InstalledArtifactRef<'_> = base.installed_artifact();
    assert_eq!((owner.uid(), owner.signature_sha256()), (10_321, SIGNATURE));
    assert_eq!(
        (artifact.version_code(), artifact.code_path()),
        (7, "/data/app/example/base.apk")
    );
}

#[test]
fn app_identity_serialization_stays_flat_and_schema_compatible() {
    let value = identity(10_321, SIGNATURE, 7, "/data/app/example/base.apk");
    let encoded = serde_json::to_value(&value).unwrap();
    assert_eq!(
        encoded,
        serde_json::json!({
            "uid": 10_321,
            "signature_sha256": SIGNATURE,
            "version_code": 7,
            "code_path": "/data/app/example/base.apk"
        })
    );
    let decoded: AppIdentity = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, value);
}
