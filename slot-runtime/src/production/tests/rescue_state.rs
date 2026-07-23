use std::collections::BTreeSet;

use crate::domain::{PackageKey, PackageName, UserId};
use crate::rescue::{RescueExecution, RescueStatus};
use crate::service::{PackageState, ServiceError};

#[test]
fn retired_rescue_state_cannot_be_treated_as_a_pristine_package() {
    let state = super::super::rescue::package_state(Some(RescueStatus::BaseRetired));

    assert!(matches!(state, Some(PackageState::RecoveryRequired)));
}

#[test]
fn completed_base_rescue_clears_only_its_daemon_recovery_override() {
    let rescued = PackageName::parse("com.example.rescued").unwrap();
    let still_broken = PackageName::parse("com.example.stillbroken").unwrap();
    let key = PackageKey::new(rescued.clone(), UserId::PRIMARY);
    let mut overrides = BTreeSet::from([rescued, still_broken.clone()]);

    assert_eq!(
        super::super::rescue::finish_rescue(&mut overrides, &key, RescueExecution::CompletedBase,),
        RescueExecution::CompletedBase
    );
    assert_eq!(overrides, BTreeSet::from([still_broken]));
}

#[test]
fn failed_rescue_keeps_daemon_recovery_override() {
    let package = PackageName::parse("com.example.failed").unwrap();
    let key = PackageKey::new(package.clone(), UserId::PRIMARY);
    let mut overrides = BTreeSet::from([package.clone()]);

    assert_eq!(
        super::super::rescue::finish_rescue(
            &mut overrides,
            &key,
            RescueExecution::RecoveryRequired,
        ),
        RescueExecution::RecoveryRequired
    );
    assert_eq!(overrides, BTreeSet::from([package]));
}

#[test]
fn ordinary_daemon_rejects_unknown_rescue_before_runtime_mutation() {
    assert_eq!(
        super::super::rescue::require_authorized(Ok(false)),
        Err(ServiceError::NotFound)
    );
}

#[test]
fn ordinary_daemon_maps_damaged_rescue_evidence_to_recovery() {
    let damaged = crate::rescue::RescueError::Corrupt("damaged evidence".to_owned());

    assert_eq!(
        super::super::rescue::require_authorized(Err(damaged)),
        Err(ServiceError::RecoveryRequired)
    );
}
