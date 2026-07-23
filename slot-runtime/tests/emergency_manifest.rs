#![doc = "Independent tests for the boot emergency manifest and its secure store."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture setup must abort the individual test"
)]

use std::fs;
use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::sync::{Arc, Barrier};
use std::thread;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{BootId, PackageName};
use uclone_slot_runtime::emergency_manifest::{
    ContainmentObligation, DiscoveryIntegrity, EmergencyManifestError, EmergencyManifestStore,
    EmergencyManifestV1, OverallDisposition, PackageContainment, RuntimeOwnerProof,
    RuntimeOwnerRole,
};

fn boot(value: &str) -> BootId {
    BootId::parse(value).unwrap()
}

fn package(value: &str, obligation: ContainmentObligation) -> PackageContainment {
    PackageContainment::new(PackageName::parse(value).unwrap(), obligation)
}

fn package_at(value: &str, epoch: u64, obligation: ContainmentObligation) -> PackageContainment {
    PackageContainment::with_enrollment_epoch(PackageName::parse(value).unwrap(), epoch, obligation)
}

fn runtime_owner(boot_id: &str) -> RuntimeOwnerProof {
    RuntimeOwnerProof::new(
        boot(boot_id),
        4242,
        81_000,
        RuntimeOwnerRole::Ucloned,
        73,
        9_001,
    )
}

fn invalid_runtime_owner_proofs() -> [RuntimeOwnerProof; 4] {
    [
        RuntimeOwnerProof::new(
            boot("boot-00000001"),
            0,
            81_000,
            RuntimeOwnerRole::Ucloned,
            73,
            9_001,
        ),
        RuntimeOwnerProof::new(
            boot("boot-00000001"),
            4242,
            0,
            RuntimeOwnerRole::Ucloned,
            73,
            9_001,
        ),
        RuntimeOwnerProof::new(
            boot("boot-00000001"),
            4242,
            81_000,
            RuntimeOwnerRole::Ucloned,
            0,
            9_001,
        ),
        RuntimeOwnerProof::new(
            boot("boot-00000001"),
            4242,
            81_000,
            RuntimeOwnerRole::Ucloned,
            73,
            0,
        ),
    ]
}

fn manifest_with_integrity(
    boot_id: &str,
    generation: u64,
    discovery_integrity: DiscoveryIntegrity,
    packages: Vec<PackageContainment>,
) -> Result<EmergencyManifestV1, EmergencyManifestError> {
    let needs_owner = packages
        .iter()
        .any(|entry| entry.obligation() == ContainmentObligation::RuntimeOwned);
    EmergencyManifestV1::with_context(
        boot(boot_id),
        generation,
        discovery_integrity,
        needs_owner.then(|| runtime_owner(boot_id)),
        packages,
    )
}

fn store() -> (TempDir, EmergencyManifestStore) {
    let root = TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let store = EmergencyManifestStore::new(root.path()).unwrap();
    (root, store)
}

#[test]
fn derives_sorted_packages_and_strongest_overall_disposition() {
    let manifest = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![
            package("com.example.zed", ContainmentObligation::BaseRetired),
            package("com.example.alpha", ContainmentObligation::Held),
        ],
    )
    .unwrap();

    assert_eq!(manifest.overall_disposition(), OverallDisposition::Held);
    assert_eq!(
        manifest.packages().first().unwrap().package().as_str(),
        "com.example.alpha"
    );
}

#[test]
fn same_boot_containment_transition_matrix_is_complete() {
    use ContainmentObligation::{BaseRetired, Held, HeldRecovery, NotManaged, RuntimeOwned};

    let obligations = [Held, HeldRecovery, RuntimeOwned, BaseRetired, NotManaged];
    let expected = [
        [true, true, true, true, false],
        [false, true, true, true, false],
        [true, true, true, false, false],
        [false, false, false, true, false],
        [false, false, false, false, true],
    ];

    for (previous, row) in obligations.into_iter().zip(expected) {
        for (next, allowed) in obligations.into_iter().zip(row) {
            assert_eq!(
                previous.can_transition_same_boot_to(next),
                allowed,
                "unexpected same-epoch transition {} -> {}",
                previous.as_str(),
                next.as_str()
            );
        }
    }
}

#[test]
fn new_boot_containment_transition_matrix_is_stricter() {
    use ContainmentObligation::{BaseRetired, Held, HeldRecovery, NotManaged, RuntimeOwned};

    let obligations = [Held, HeldRecovery, RuntimeOwned, BaseRetired, NotManaged];
    let expected = [
        [true, true, false, false, false],
        [false, true, false, false, false],
        [true, true, false, false, false],
        [false, false, false, true, false],
        [false, false, false, false, true],
    ];

    for (previous, row) in obligations.into_iter().zip(expected) {
        for (next, allowed) in obligations.into_iter().zip(row) {
            assert_eq!(
                previous.can_transition_new_boot_to(next),
                allowed,
                "unexpected new-boot transition {} -> {}",
                previous.as_str(),
                next.as_str()
            );
        }
    }
}

#[test]
fn derives_fail_closed_overall_disposition_priority() {
    use ContainmentObligation::{BaseRetired, Held, HeldRecovery, NotManaged, RuntimeOwned};

    let priorities = [
        (NotManaged, OverallDisposition::NotManaged),
        (BaseRetired, OverallDisposition::BaseRetired),
        (RuntimeOwned, OverallDisposition::RuntimeOwned),
        (Held, OverallDisposition::Held),
        (HeldRecovery, OverallDisposition::HeldRecovery),
    ];

    for (weaker_index, (weaker, _)) in priorities.into_iter().enumerate() {
        for (stronger, expected) in priorities.iter().copied().skip(weaker_index) {
            let manifest = manifest_with_integrity(
                "boot-00000001",
                2,
                DiscoveryIntegrity::Complete,
                vec![
                    package("com.example.alpha", weaker),
                    package("com.example.zed", stronger),
                ],
            )
            .unwrap();
            assert_eq!(
                manifest.overall_disposition(),
                expected,
                "unexpected aggregate for {} plus {}",
                weaker.as_str(),
                stronger.as_str()
            );
        }
    }
}

#[test]
fn untrusted_discovery_imposes_held_recovery_even_without_packages() {
    let empty = manifest_with_integrity(
        "boot-00000001",
        1,
        DiscoveryIntegrity::Untrusted,
        Vec::new(),
    )
    .unwrap();
    assert_eq!(
        empty.overall_disposition(),
        OverallDisposition::HeldRecovery
    );

    let retired = manifest_with_integrity(
        "boot-00000001",
        1,
        DiscoveryIntegrity::Untrusted,
        vec![package(
            "com.example.app",
            ContainmentObligation::BaseRetired,
        )],
    )
    .unwrap();
    assert_eq!(
        retired.overall_disposition(),
        OverallDisposition::HeldRecovery
    );

    let mixed = manifest_with_integrity(
        "boot-00000001",
        1,
        DiscoveryIntegrity::Untrusted,
        vec![
            package("com.example.alpha", ContainmentObligation::Held),
            package("com.example.zed", ContainmentObligation::BaseRetired),
        ],
    )
    .unwrap();
    assert_eq!(
        mixed.overall_disposition(),
        OverallDisposition::HeldRecovery
    );
}

#[test]
fn runtime_owned_requires_an_exact_owner_proof_and_rejects_stale_proofs() {
    let packages = vec![package(
        "com.example.app",
        ContainmentObligation::RuntimeOwned,
    )];
    let missing = EmergencyManifestV1::with_context(
        boot("boot-00000001"),
        2,
        DiscoveryIntegrity::Complete,
        None,
        packages.clone(),
    )
    .unwrap_err();
    assert!(missing.to_string().contains("owner proof"));

    let stale_boot = EmergencyManifestV1::with_context(
        boot("boot-00000001"),
        2,
        DiscoveryIntegrity::Complete,
        Some(runtime_owner("boot-00000002")),
        packages.clone(),
    )
    .unwrap_err();
    assert!(stale_boot.to_string().contains("boot"));

    for invalid in invalid_runtime_owner_proofs() {
        assert!(
            EmergencyManifestV1::with_context(
                boot("boot-00000001"),
                2,
                DiscoveryIntegrity::Complete,
                Some(invalid),
                packages.clone(),
            )
            .is_err()
        );
    }

    let delegated_held = EmergencyManifestV1::with_context(
        boot("boot-00000001"),
        2,
        DiscoveryIntegrity::Complete,
        Some(runtime_owner("boot-00000001")),
        vec![package("com.example.app", ContainmentObligation::Held)],
    )
    .unwrap();
    assert!(delegated_held.runtime_owner_proof().is_some());

    let terminal_proof = EmergencyManifestV1::with_context(
        boot("boot-00000001"),
        2,
        DiscoveryIntegrity::Complete,
        Some(runtime_owner("boot-00000001")),
        vec![package(
            "com.example.app",
            ContainmentObligation::BaseRetired,
        )],
    )
    .unwrap_err();
    assert!(terminal_proof.to_string().contains("owner proof"));

    let untrusted_owner = EmergencyManifestV1::with_context(
        boot("boot-00000001"),
        2,
        DiscoveryIntegrity::Untrusted,
        Some(runtime_owner("boot-00000001")),
        vec![package("com.example.app", ContainmentObligation::Held)],
    )
    .unwrap_err();
    assert!(untrusted_owner.to_string().contains("untrusted discovery"));

    let untrusted_runtime_owned =
        manifest_with_integrity("boot-00000001", 2, DiscoveryIntegrity::Untrusted, packages)
            .unwrap_err();
    assert!(
        untrusted_runtime_owned
            .to_string()
            .contains("untrusted discovery")
    );
}

#[test]
fn overall_disposition_is_not_weakened_by_a_later_not_managed_entry() {
    let manifest = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![
            package("com.example.alpha", ContainmentObligation::BaseRetired),
            package("com.example.zed", ContainmentObligation::NotManaged),
        ],
    )
    .unwrap();
    assert_eq!(
        manifest.overall_disposition(),
        OverallDisposition::BaseRetired
    );
}

#[test]
fn new_boot_generation_one_rejects_runtime_owned_even_with_an_owner_proof() {
    let (_root, store) = store();
    let unsupported_root = manifest_with_integrity(
        "boot-00000001",
        1,
        DiscoveryIntegrity::Complete,
        vec![package(
            "com.example.app",
            ContainmentObligation::RuntimeOwned,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&unsupported_root),
        Err(EmergencyManifestError::IllegalTransition { .. })
    ));

    let held = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.app", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&held).unwrap();
    let unsupported_next_boot = manifest_with_integrity(
        "boot-00000002",
        1,
        DiscoveryIntegrity::Complete,
        vec![package(
            "com.example.app",
            ContainmentObligation::RuntimeOwned,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&unsupported_next_boot),
        Err(EmergencyManifestError::IllegalTransition { .. })
    ));
}

#[test]
fn rejects_duplicate_unknown_and_forged_overall_records() {
    let duplicate = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![
            package("com.example.app", ContainmentObligation::Held),
            package("com.example.app", ContainmentObligation::HeldRecovery),
        ],
    )
    .unwrap_err();
    assert!(duplicate.to_string().contains("duplicate package"));

    let unknown = serde_json::from_str::<EmergencyManifestV1>(
        r#"{"schema_version":3,"boot_id":"boot-00000001","generation":1,"discovery_integrity":"complete","runtime_owner_proof":null,"overall_disposition":"not_managed","packages":[],"extra":true}"#,
    );
    assert!(unknown.is_err());

    let forged = serde_json::from_str::<EmergencyManifestV1>(
        r#"{"schema_version":3,"boot_id":"boot-00000001","generation":1,"discovery_integrity":"complete","runtime_owner_proof":null,"overall_disposition":"held","packages":[]}"#,
    )
    .unwrap_err();
    assert!(forged.to_string().contains("overall disposition"));
}

#[test]
fn serialized_manifest_requires_discovery_integrity_and_owner_proof_fields() {
    let missing_discovery = serde_json::from_str::<EmergencyManifestV1>(
        r#"{"schema_version":3,"boot_id":"boot-00000001","generation":1,"runtime_owner_proof":null,"overall_disposition":"not_managed","packages":[]}"#,
    );
    assert!(missing_discovery.is_err());

    let owned = manifest_with_integrity(
        "boot-00000001",
        2,
        DiscoveryIntegrity::Complete,
        vec![package(
            "com.example.app",
            ContainmentObligation::RuntimeOwned,
        )],
    )
    .unwrap();
    let encoded = serde_json::to_string(&owned).unwrap();
    assert!(encoded.contains(r#""schema_version":3"#));
    assert!(encoded.contains(r#""role":"ucloned""#));
    assert!(encoded.contains(r#""lock_device":73"#));
    assert_eq!(
        serde_json::from_str::<EmergencyManifestV1>(&encoded).unwrap(),
        owned
    );
}

#[test]
fn legacy_v2_golden_is_verified_then_upgraded_fail_closed_in_memory() {
    let legacy = r#"{"schema_version":2,"boot_id":"boot-00000001","generation":7,"overall_disposition":"held","packages":[{"package":"com.example.app","enrollment_epoch":4,"obligation":"held"}]}"#;
    let upgraded = serde_json::from_str::<EmergencyManifestV1>(legacy).unwrap();

    assert_eq!(upgraded.schema_version(), 3);
    assert_eq!(
        upgraded.discovery_integrity(),
        DiscoveryIntegrity::Untrusted
    );
    assert!(upgraded.runtime_owner_proof().is_none());
    assert_eq!(
        upgraded.overall_disposition(),
        OverallDisposition::HeldRecovery
    );
    assert_eq!(
        upgraded.obligation_for(&PackageName::parse("com.example.app").unwrap()),
        Some(ContainmentObligation::Held)
    );

    let rewritten = serde_json::to_string(&upgraded).unwrap();
    assert!(rewritten.contains(r#""schema_version":3"#));
    assert!(rewritten.contains(r#""discovery_integrity":"untrusted""#));
    assert!(rewritten.contains(r#""runtime_owner_proof":null"#));
}

#[test]
fn store_loads_committed_v2_then_next_commit_rewrites_v3() {
    let (_root, store) = store();
    let legacy = r#"{"schema_version":2,"boot_id":"boot-00000001","generation":7,"overall_disposition":"held","packages":[{"package":"com.example.app","enrollment_epoch":4,"obligation":"held"}]}"#;
    fs::write(store.manifest_path(), legacy).unwrap();
    fs::set_permissions(store.manifest_path(), fs::Permissions::from_mode(0o600)).unwrap();

    let upgraded = store.load().unwrap().unwrap();
    assert_eq!(upgraded.schema_version(), 3);
    assert_eq!(
        upgraded.overall_disposition(),
        OverallDisposition::HeldRecovery
    );

    let next = EmergencyManifestV1::new(
        boot("boot-00000001"),
        8,
        vec![package_at(
            "com.example.app",
            4,
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    store.commit(&next).unwrap();
    let rewritten = fs::read_to_string(store.manifest_path()).unwrap();
    assert!(rewritten.contains(r#""schema_version":3"#));
    assert!(rewritten.contains(r#""discovery_integrity":"complete""#));
}

#[test]
fn legacy_v2_reader_rejects_forged_aggregate_runtime_owned_and_new_fields() {
    let forged_aggregate = r#"{"schema_version":2,"boot_id":"boot-00000001","generation":1,"overall_disposition":"not_managed","packages":[{"package":"com.example.app","enrollment_epoch":1,"obligation":"held"}]}"#;
    assert!(serde_json::from_str::<EmergencyManifestV1>(forged_aggregate).is_err());

    let runtime_owned = r#"{"schema_version":2,"boot_id":"boot-00000001","generation":1,"overall_disposition":"runtime_owned","packages":[{"package":"com.example.app","enrollment_epoch":1,"obligation":"runtime_owned"}]}"#;
    let runtime_error = serde_json::from_str::<EmergencyManifestV1>(runtime_owned).unwrap_err();
    assert!(runtime_error.to_string().contains("RuntimeOwned"));

    let smuggled_v3_field = r#"{"schema_version":2,"boot_id":"boot-00000001","generation":1,"discovery_integrity":"complete","runtime_owner_proof":null,"overall_disposition":"not_managed","packages":[]}"#;
    assert!(serde_json::from_str::<EmergencyManifestV1>(smuggled_v3_field).is_err());
}

#[test]
fn versioned_reader_rejects_unsorted_v2_and_incomplete_v3_records() {
    let unsorted_v2 = r#"{"schema_version":2,"boot_id":"boot-00000001","generation":1,"overall_disposition":"held","packages":[{"package":"com.example.zed","enrollment_epoch":1,"obligation":"held"},{"package":"com.example.alpha","enrollment_epoch":1,"obligation":"held"}]}"#;
    let sort_error = serde_json::from_str::<EmergencyManifestV1>(unsorted_v2).unwrap_err();
    assert!(sort_error.to_string().contains("sorted"));

    let incomplete_v3 = r#"{"schema_version":3,"boot_id":"boot-00000001","generation":1,"overall_disposition":"not_managed","packages":[]}"#;
    assert!(serde_json::from_str::<EmergencyManifestV1>(incomplete_v3).is_err());
}

#[test]
fn commits_monotonic_same_boot_and_allows_only_new_boot_generation_one() {
    let (_root, store) = store();
    let first = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.app", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&first).unwrap();
    let second = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package(
            "com.example.app",
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    store.commit(&second).unwrap();

    let stale = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package(
            "com.example.app",
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&stale),
        Err(EmergencyManifestError::StaleGeneration { .. })
    ));

    let wrong_new_boot = EmergencyManifestV1::new(boot("boot-00000002"), 2, Vec::new()).unwrap();
    assert!(matches!(
        store.commit(&wrong_new_boot),
        Err(EmergencyManifestError::StaleGeneration { .. })
    ));
    let new_boot = EmergencyManifestV1::new(
        boot("boot-00000002"),
        1,
        vec![package(
            "com.example.app",
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    store.commit(&new_boot).unwrap();
    assert_eq!(
        store.load_for_boot(&boot("boot-00000002")).unwrap(),
        Some(new_boot)
    );
}

#[test]
fn new_boot_preserves_package_entries_tombstones_and_epoch_fences() {
    let (_root, store) = store();
    let tombstone = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package_at(
            "com.example.app",
            4,
            ContainmentObligation::NotManaged,
        )],
    )
    .unwrap();
    store.commit(&tombstone).unwrap();

    let missing = EmergencyManifestV1::new(boot("boot-00000002"), 1, Vec::new()).unwrap();
    assert!(matches!(
        store.commit(&missing),
        Err(EmergencyManifestError::IllegalTransition { .. })
    ));

    let reset_epoch = EmergencyManifestV1::new(
        boot("boot-00000002"),
        1,
        vec![package(
            "com.example.app",
            ContainmentObligation::NotManaged,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&reset_epoch),
        Err(EmergencyManifestError::EnrollmentEpoch { .. })
    ));

    let forbidden_reenrollment = EmergencyManifestV1::new(
        boot("boot-00000002"),
        1,
        vec![package_at(
            "com.example.app",
            5,
            ContainmentObligation::Held,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&forbidden_reenrollment),
        Err(EmergencyManifestError::EnrollmentEpoch { .. })
    ));

    let preserved = EmergencyManifestV1::new(
        boot("boot-00000002"),
        1,
        vec![
            package_at("com.example.app", 4, ContainmentObligation::NotManaged),
            package("com.example.new", ContainmentObligation::Held),
        ],
    )
    .unwrap();
    store.commit(&preserved).unwrap();
}

#[test]
fn new_boot_replaces_runtime_owned_with_a_held_obligation() {
    let (_root, store) = store();
    let held = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.app", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&held).unwrap();
    let owned = manifest_with_integrity(
        "boot-00000001",
        2,
        DiscoveryIntegrity::Complete,
        vec![package(
            "com.example.app",
            ContainmentObligation::RuntimeOwned,
        )],
    )
    .unwrap();
    store.commit(&owned).unwrap();

    let next_boot = EmergencyManifestV1::new(
        boot("boot-00000002"),
        1,
        vec![package("com.example.app", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&next_boot).unwrap();
}

#[test]
fn rejects_illegal_transition_and_stale_boot_read() {
    let (_root, store) = store();
    let first = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package(
            "com.example.app",
            ContainmentObligation::BaseRetired,
        )],
    )
    .unwrap();
    store.commit(&first).unwrap();
    let illegal = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.app",
            1,
            ContainmentObligation::NotManaged,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&illegal),
        Err(EmergencyManifestError::IllegalTransition { .. })
    ));
    assert!(matches!(
        store.load_for_boot(&boot("boot-00000002")),
        Err(EmergencyManifestError::StaleBoot { .. })
    ));
}

#[test]
fn generic_same_boot_transition_rejects_not_managed_without_typed_abort_proof() {
    for initial in [
        ContainmentObligation::Held,
        ContainmentObligation::HeldRecovery,
    ] {
        let (_root, store) = store();
        let first = EmergencyManifestV1::new(
            boot("boot-00000001"),
            1,
            vec![package("com.example.app", initial)],
        )
        .unwrap();
        store.commit(&first).unwrap();
        let aborted = EmergencyManifestV1::new(
            boot("boot-00000001"),
            2,
            vec![package(
                "com.example.app",
                ContainmentObligation::NotManaged,
            )],
        )
        .unwrap();
        assert!(matches!(
            store.commit(&aborted),
            Err(EmergencyManifestError::IllegalTransition { .. })
        ));
    }
}

#[test]
fn permits_a_new_package_to_join_the_same_boot_generation() {
    let (_root, store) = store();
    let first = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.alpha", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&first).unwrap();
    let second = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![
            package("com.example.alpha", ContainmentObligation::Held),
            package("com.example.beta", ContainmentObligation::Held),
        ],
    )
    .unwrap();
    store.commit(&second).unwrap();
    assert_eq!(store.load().unwrap(), Some(second));
}

#[test]
fn requires_an_epoch_fenced_reenrollment_after_base_retired() {
    let (_root, store) = store();
    let retired = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package(
            "com.example.alpha",
            ContainmentObligation::BaseRetired,
        )],
    )
    .unwrap();
    store.commit(&retired).unwrap();

    let stale_reenrollment = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.alpha",
            1,
            ContainmentObligation::Held,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&stale_reenrollment),
        Err(EmergencyManifestError::EnrollmentEpoch { .. })
    ));

    let reenrollment = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.alpha",
            2,
            ContainmentObligation::Held,
        )],
    )
    .unwrap();
    store.commit(&reenrollment).unwrap();
    assert_eq!(store.load().unwrap(), Some(reenrollment));
}

#[test]
fn terminal_entries_reenroll_only_held_at_the_next_epoch() {
    use ContainmentObligation::{BaseRetired, Held, HeldRecovery, NotManaged, RuntimeOwned};

    for (index, terminal) in [BaseRetired, NotManaged].into_iter().enumerate() {
        let (_root, terminal_store) = store();
        let package_name = format!("com.example.terminal{index}");
        let terminal_manifest = EmergencyManifestV1::new(
            boot("boot-00000001"),
            1,
            vec![package(&package_name, terminal)],
        )
        .unwrap();
        terminal_store.commit(&terminal_manifest).unwrap();

        for forbidden in [RuntimeOwned, BaseRetired, NotManaged] {
            let candidate = manifest_with_integrity(
                "boot-00000001",
                2,
                DiscoveryIntegrity::Complete,
                vec![package_at(&package_name, 2, forbidden)],
            )
            .unwrap();
            assert!(matches!(
                terminal_store.commit(&candidate),
                Err(EmergencyManifestError::IllegalTransition { .. })
            ));
        }

        for allowed in [Held, HeldRecovery] {
            let (_fresh_root, fresh_store) = store();
            fresh_store.commit(&terminal_manifest).unwrap();
            let reenrolled = EmergencyManifestV1::new(
                boot("boot-00000001"),
                2,
                vec![package_at(&package_name, 2, allowed)],
            )
            .unwrap();
            fresh_store.commit(&reenrolled).unwrap();
        }
    }
}

#[test]
fn runtime_owned_requires_an_existing_same_boot_enrollment() {
    let (_root, store) = store();
    let first = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.alpha", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&first).unwrap();

    let owned = manifest_with_integrity(
        "boot-00000001",
        2,
        DiscoveryIntegrity::Complete,
        vec![package(
            "com.example.alpha",
            ContainmentObligation::RuntimeOwned,
        )],
    )
    .unwrap();
    store.commit(&owned).unwrap();

    let held_again = EmergencyManifestV1::new(
        boot("boot-00000001"),
        3,
        vec![package("com.example.alpha", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&held_again).unwrap();

    let new_package_claim = manifest_with_integrity(
        "boot-00000001",
        4,
        DiscoveryIntegrity::Complete,
        vec![
            package("com.example.alpha", ContainmentObligation::Held),
            package("com.example.beta", ContainmentObligation::RuntimeOwned),
        ],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&new_package_claim),
        Err(EmergencyManifestError::IllegalTransition { .. })
    ));
}

#[test]
fn preserves_not_managed_tombstone_and_fences_reenrollment_epoch() {
    let (_root, store) = store();
    let not_managed = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package(
            "com.example.alpha",
            ContainmentObligation::NotManaged,
        )],
    )
    .unwrap();
    store.commit(&not_managed).unwrap();

    let deleted = EmergencyManifestV1::new(boot("boot-00000001"), 2, Vec::new()).unwrap();
    assert!(matches!(
        store.commit(&deleted),
        Err(EmergencyManifestError::IllegalTransition { .. })
    ));

    let stale_reenrollment = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.alpha",
            1,
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    assert!(matches!(
        store.commit(&stale_reenrollment),
        Err(EmergencyManifestError::EnrollmentEpoch { .. })
    ));

    let reenrollment = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package_at(
            "com.example.alpha",
            2,
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    store.commit(&reenrollment).unwrap();
    assert_eq!(store.load().unwrap(), Some(reenrollment));
}

#[test]
fn ignores_partial_temp_and_fails_closed_on_corrupt_committed_record() {
    let (root, store) = store();
    let manifest = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.app", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&manifest).unwrap();
    let temporary = root.path().join(".manifest.json.tmp-orphan");
    fs::write(&temporary, b"partial").unwrap();
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(store.load().unwrap(), Some(manifest));

    fs::write(store.manifest_path(), b"{\"partial\":true}").unwrap();
    let error = store.load().unwrap_err();
    assert!(matches!(error, EmergencyManifestError::Corrupt(_)));
}

#[test]
fn publishes_root_owned_record_with_bounded_permissions() {
    let (_root, store) = store();
    let manifest = EmergencyManifestV1::new(boot("boot-00000001"), 1, Vec::new()).unwrap();
    store.commit(&manifest).unwrap();
    let metadata = fs::symlink_metadata(store.manifest_path()).unwrap();
    assert_eq!(metadata.mode() & 0o7777, 0o600);
    assert_eq!(metadata.nlink(), 1);
    assert_eq!(
        metadata.uid(),
        fs::symlink_metadata(store.root()).unwrap().uid()
    );
}

#[test]
fn creates_a_fixed_root_owned_commit_lock() {
    let (root, _store) = store();
    let lock = root.path().join(".manifest.lock");
    let metadata = fs::symlink_metadata(lock).unwrap();
    assert_eq!(metadata.mode() & 0o7777, 0o600);
    assert_eq!(metadata.nlink(), 1);
    assert_eq!(metadata.len(), 0);
    assert_eq!(
        metadata.uid(),
        fs::symlink_metadata(root.path()).unwrap().uid()
    );
}

#[test]
fn serializes_two_same_generation_writers_with_one_winner() {
    let (_root, store) = store();
    let first = EmergencyManifestV1::new(
        boot("boot-00000001"),
        1,
        vec![package("com.example.alpha", ContainmentObligation::Held)],
    )
    .unwrap();
    store.commit(&first).unwrap();
    let candidate = EmergencyManifestV1::new(
        boot("boot-00000001"),
        2,
        vec![package(
            "com.example.alpha",
            ContainmentObligation::HeldRecovery,
        )],
    )
    .unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let left = store.clone();
    let right = store.clone();
    let left_barrier = Arc::clone(&barrier);
    let right_barrier = Arc::clone(&barrier);
    let left_candidate = candidate.clone();
    let right_candidate = candidate.clone();
    let left_thread = thread::spawn(move || {
        left_barrier.wait();
        left.commit(&left_candidate)
    });
    let right_thread = thread::spawn(move || {
        right_barrier.wait();
        right.commit(&right_candidate)
    });
    barrier.wait();
    let left_result = left_thread.join().unwrap();
    let right_result = right_thread.join().unwrap();
    assert_eq!(
        u8::from(left_result.is_ok()) + u8::from(right_result.is_ok()),
        1
    );
    assert!(matches!(
        left_result.err().or_else(|| right_result.err()),
        Some(EmergencyManifestError::StaleGeneration { .. })
    ));
    assert_eq!(store.load().unwrap(), Some(candidate));
}
