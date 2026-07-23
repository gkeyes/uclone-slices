use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

#[path = "package_state_matrix/fixture.rs"]
mod fixture;
#[path = "package_state_matrix/fixture_cases.rs"]
mod fixture_cases;
#[path = "package_state_matrix/fixture_data.rs"]
mod fixture_data;
#[path = "package_state_matrix/probe.rs"]
mod matrix_probe;

use super::slot_lifecycle_support::ready_package;
use crate::domain::{GateSnapshot, PackageEnabledState};
use crate::service::{ObservedGateState, PackageState};
use matrix_probe::MatrixProbe;

#[test]
fn package_state_matrix_rows() {
    for row in fixture::rows() {
        let mut case = fixture::build(row);
        let actual = super::super::state::load(&case.stores, &mut case.probe, &case.key);
        fixture::assert_expected(actual, case.expected);
    }
}

#[test]
fn package_state_matrix_lifecycle_drift_is_not_promoted_to_recovery_today() {
    let mut case = fixture::build(fixture::Row::LifecycleDrift);
    let actual = super::super::state::load(&case.stores, &mut case.probe, &case.key);
    fixture::assert_expected(actual, fixture::Expected::DriftReady);
}

#[test]
#[allow(
    clippy::panic,
    reason = "matrix assertions use explicit impossible-state branches"
)]
fn package_state_matrix_gate_combinations_preserve_observed_facts() {
    for enabled_state in [
        PackageEnabledState::Default,
        PackageEnabledState::Enabled,
        PackageEnabledState::Disabled,
        PackageEnabledState::DisabledUser,
        PackageEnabledState::DisabledUntilUsed,
    ] {
        for suspended in [false, true] {
            let root = tempdir().unwrap();
            fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
            let (stores, key, _) = ready_package(root.path());
            let mut package_probe =
                MatrixProbe::healthy(GateSnapshot::new(enabled_state, suspended));
            let loaded = super::super::state::load(&stores, &mut package_probe, &key).unwrap();
            let PackageState::Ready(snapshot) = loaded else {
                panic!("gate combination was not ready");
            };
            assert_eq!(
                snapshot.gate(),
                ObservedGateState::new(
                    matches!(
                        enabled_state,
                        PackageEnabledState::Default | PackageEnabledState::Enabled
                    ),
                    suspended,
                )
            );
        }
    }
}
