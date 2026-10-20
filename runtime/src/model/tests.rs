#![allow(clippy::unwrap_used)]

use super::*;

fn package() -> PackageName {
    PackageName::new("com.example.app").unwrap()
}

fn identity() -> PackageIdentity {
    PackageIdentity::new(10_000, "/data/app/example/base.apk", 1, 2).unwrap()
}

#[test]
fn identifiers_only_enforce_operational_grammar() {
    assert!(PackageName::new("com.example.app").is_ok());
    assert!(PackageName::new("single").is_err());
    assert!(PackageName::new("../app").is_err());
    assert!(DisplayName::new("Work profile").is_ok());
    assert!(DisplayName::new("\n").is_err());
}

#[test]
fn restore_policy_accepts_single_wildcards_and_rejects_unsafe_or_overlapping_paths() {
    let valid = RestorePolicy::PreservePaths {
        paths: vec![RestorePath {
            domain: ArchiveDomain::Ce,
            path: "files/Dolphin/*/Paks".to_owned(),
        }],
    };
    assert!(valid.validate().is_ok());

    for path in ["/data/user/0/app", "../files", "files/**/Paks"] {
        assert_eq!(
            RestorePolicy::PreservePaths {
                paths: vec![RestorePath {
                    domain: ArchiveDomain::Ce,
                    path: path.to_owned(),
                }],
            }
            .validate(),
            Err(ModelError::InvalidState),
        );
    }
    let overlapping = RestorePolicy::PreservePaths {
        paths: vec![
            RestorePath {
                domain: ArchiveDomain::Ce,
                path: "files/game".to_owned(),
            },
            RestorePath {
                domain: ArchiveDomain::Ce,
                path: "files/game/Paks".to_owned(),
            },
        ],
    };
    assert_eq!(overlapping.validate(), Err(ModelError::InvalidState));
}

#[test]
fn aggregate_owns_slot_lifecycle() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());
    let slot = aggregate
        .reserve_slot(DisplayName::new("Work").unwrap())
        .unwrap();
    let slot_id = slot.id().clone();
    aggregate.begin_creation(&slot, false).unwrap();
    aggregate.finish_creation(slot).unwrap();
    aggregate.begin_activation(&slot_id).unwrap();
    aggregate.finish_activation(&slot_id).unwrap();

    assert_eq!(aggregate.active_slot(), &slot_id);
    assert_eq!(aggregate.snapshot().unwrap().slots.len(), 2);
}

#[test]
fn reboot_launch_defaults_off_and_survives_round_trip() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());

    assert!(!aggregate.launch_after_reboot());
    aggregate.set_launch_after_reboot(true).unwrap();

    let encoded = serde_json::to_string(&aggregate).unwrap();
    let restored: PackageAggregate = serde_json::from_str(&encoded).unwrap();
    assert!(restored.launch_after_reboot());
    assert!(restored.snapshot().unwrap().launch_after_reboot);
}

#[test]
fn persisted_aggregate_without_reboot_launch_setting_migrates_off() {
    let aggregate = PackageAggregate::enrolled(package(), identity());
    let mut legacy = serde_json::to_value(aggregate).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("launch_after_reboot");

    let restored: PackageAggregate = serde_json::from_value(legacy).unwrap();

    assert!(!restored.launch_after_reboot());
    assert!(!restored.snapshot().unwrap().launch_after_reboot);
}

#[test]
fn aggregate_wire_format_stays_independent_from_the_v1_binding_sidecar() {
    let aggregate = PackageAggregate::enrolled(package(), identity());

    let encoded = serde_json::to_value(aggregate).unwrap();

    let fields = encoded.as_object().unwrap();
    assert!(!fields.contains_key("binding"));
    assert!(!fields.contains_key("signing"));
    assert!(!fields.contains_key("binding_state"));
}

#[test]
fn signing_lineages_accept_verified_rotation_in_either_direction() {
    let old = SigningIdentity::new(SigningKind::Lineage, vec!["a".repeat(64)]).unwrap();
    let rotated =
        SigningIdentity::new(SigningKind::Lineage, vec!["a".repeat(64), "b".repeat(64)]).unwrap();

    assert!(old.is_compatible_with(&rotated));
    assert!(rotated.is_compatible_with(&old));
    let newest_first =
        SigningIdentity::new(SigningKind::Lineage, vec!["b".repeat(64), "a".repeat(64)]).unwrap();
    assert!(old.is_compatible_with(&newest_first));
}

#[test]
fn multiple_signers_require_a_sorted_exact_set() {
    let first =
        SigningIdentity::new(SigningKind::Multiple, vec!["a".repeat(64), "b".repeat(64)]).unwrap();
    let same =
        SigningIdentity::new(SigningKind::Multiple, vec!["a".repeat(64), "b".repeat(64)]).unwrap();
    let different =
        SigningIdentity::new(SigningKind::Multiple, vec!["a".repeat(64), "c".repeat(64)]).unwrap();

    assert!(first.is_compatible_with(&same));
    assert!(!first.is_compatible_with(&different));
    assert!(
        SigningIdentity::new(SigningKind::Multiple, vec!["b".repeat(64), "a".repeat(64)],).is_err()
    );
    assert!(SigningIdentity::new(SigningKind::Lineage, vec!["A".repeat(64)]).is_err());
    assert!(SigningIdentity::new(SigningKind::Lineage, vec!["a".repeat(63)]).is_err());
}

#[test]
fn invalid_persisted_relationships_are_rejected() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());
    aggregate.active_slot = SlotId::numbered(9);

    assert_eq!(aggregate.validate(), Err(ModelError::InvalidState));
}

#[test]
fn interrupted_activation_keeps_both_directions() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());
    let slot = aggregate
        .reserve_slot(DisplayName::new("Work").unwrap())
        .unwrap();
    let slot_id = slot.id().clone();
    aggregate.begin_creation(&slot, false).unwrap();
    aggregate.finish_creation(slot).unwrap();
    aggregate.begin_activation(&slot_id).unwrap();

    let base = SlotId::base();
    assert_eq!(aggregate.pending_activation(), Some((&base, &slot_id)));
    aggregate.abort_activation(&base).unwrap();
    assert!(aggregate.snapshot().is_ok());
}

#[test]
fn rename_and_delete_preserve_slot_identity_and_monotonic_numbering() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());
    let slot = aggregate
        .reserve_slot(DisplayName::new("Work").unwrap())
        .unwrap();
    let slot_id = slot.id().clone();
    aggregate.begin_creation(&slot, false).unwrap();
    aggregate.finish_creation(slot).unwrap();

    aggregate
        .rename_slot(&slot_id, DisplayName::new("Personal").unwrap())
        .unwrap();
    assert_eq!(aggregate.snapshot().unwrap().slots[1].name, "Personal");

    aggregate.begin_deletion(&slot_id).unwrap();
    assert_eq!(aggregate.pending_deletion(), Some(&slot_id));
    assert_eq!(aggregate.snapshot(), Err(ModelError::StateConflict));
    aggregate.finish_deletion(&slot_id).unwrap();
    assert!(!aggregate.has_slot(&slot_id));

    let next = aggregate
        .reserve_slot(DisplayName::new("Next").unwrap())
        .unwrap();
    assert_eq!(next.id(), &SlotId::numbered(2));
}

#[test]
fn base_active_and_missing_slots_cannot_be_deleted() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());
    assert_eq!(
        aggregate.begin_deletion(&SlotId::base()),
        Err(ModelError::StateConflict)
    );
    assert_eq!(
        aggregate.begin_deletion(&SlotId::numbered(9)),
        Err(ModelError::SlotNotFound)
    );

    let slot = aggregate
        .reserve_slot(DisplayName::new("Work").unwrap())
        .unwrap();
    let slot_id = slot.id().clone();
    aggregate.begin_creation(&slot, false).unwrap();
    aggregate.finish_creation(slot).unwrap();
    aggregate.begin_activation(&slot_id).unwrap();
    aggregate.finish_activation(&slot_id).unwrap();

    assert_eq!(
        aggregate.begin_deletion(&slot_id),
        Err(ModelError::StateConflict)
    );
    aggregate
        .rename_slot(&slot_id, DisplayName::new("Current").unwrap())
        .unwrap();
    assert_eq!(aggregate.snapshot().unwrap().slots[1].name, "Current");
    assert_eq!(
        aggregate.rename_slot(&SlotId::base(), DisplayName::new("Root").unwrap()),
        Err(ModelError::StateConflict)
    );
}

#[test]
fn unenrollment_is_a_durable_terminal_intent() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());

    aggregate.begin_unenrollment().unwrap();

    assert!(aggregate.is_unenrolling());
    assert_eq!(aggregate.snapshot(), Err(ModelError::StateConflict));
    assert_eq!(
        aggregate.begin_unenrollment(),
        Err(ModelError::StateConflict)
    );
    let encoded = serde_json::to_string(&aggregate).unwrap();
    assert!(encoded.contains(r#""state":"unenrolling""#));
    let restored: PackageAggregate = serde_json::from_str(&encoded).unwrap();
    assert!(restored.is_unenrolling());
    assert!(restored.validate().is_ok());
}

#[test]
fn default_slot_policy_migrates_off_and_round_trips_without_changing_active_slot() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());
    let mut legacy = serde_json::to_value(&aggregate).unwrap();
    legacy.as_object_mut().unwrap().remove("default_slot");
    let migrated: PackageAggregate = serde_json::from_value(legacy).unwrap();
    assert_eq!(migrated.default_slot(), None);
    aggregate.set_default_slot(Some(SlotId::base())).unwrap();
    assert_eq!(aggregate.active_slot(), &SlotId::base());
    let restored: PackageAggregate =
        serde_json::from_str(&serde_json::to_string(&aggregate).unwrap()).unwrap();
    assert_eq!(
        restored.snapshot().unwrap().default_slot,
        Some(SlotId::base())
    );
    aggregate.set_default_slot(None).unwrap();
    assert_eq!(aggregate.default_slot(), None);
}

#[test]
fn deleting_default_slot_disables_policy_only_when_deletion_finishes() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());
    let slot = aggregate
        .reserve_slot(DisplayName::new("Work").unwrap())
        .unwrap();
    let target = slot.id().clone();
    aggregate.begin_creation(&slot, false).unwrap();
    aggregate.finish_creation(slot).unwrap();
    aggregate.set_default_slot(Some(target.clone())).unwrap();
    aggregate.begin_deletion(&target).unwrap();
    assert_eq!(aggregate.default_slot(), Some(&target));
    aggregate.finish_deletion(&target).unwrap();
    assert_eq!(aggregate.default_slot(), None);
    assert_eq!(aggregate.active_slot(), &SlotId::base());
}

#[test]
fn invalid_default_slot_is_rejected_without_replacing_existing_policy() {
    let mut aggregate = PackageAggregate::enrolled(package(), identity());
    aggregate.set_default_slot(Some(SlotId::base())).unwrap();
    assert_eq!(
        aggregate.set_default_slot(Some(SlotId::numbered(999))),
        Err(ModelError::SlotNotFound)
    );
    assert_eq!(aggregate.default_slot(), Some(&SlotId::base()));
}
