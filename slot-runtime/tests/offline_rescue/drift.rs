use uclone_slot_runtime::domain::{
    DataInodes, GateSnapshot, PackageEnabledState, PackageObservation,
};
use uclone_slot_runtime::rescue::{RescueExecution, RescuePhase};

use super::support::{CrashOnce, FakeMetadata, Fixture, SIGNATURE, base_inodes, identity};

const OTHER_SIGNATURE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[test]
fn uid_or_signing_identity_change_is_quarantined_and_gated() {
    for live_identity in [
        identity(10_322, SIGNATURE, 1),
        identity(10_321, OTHER_SIGNATURE, 1),
    ] {
        let fixture = Fixture::new();
        let mut backend = fixture.backend();
        backend.set_observation(observation(live_identity, base_inodes(), false));
        let mut platform = fixture.platform(backend, FakeMetadata::default(), CrashOnce::never());

        assert_eq!(
            platform.rescue_to_base(&Fixture::key()),
            RescueExecution::Quarantined
        );
        let (backend, metadata, _) = platform.into_dependencies();
        assert_eq!(backend.emergency_gate_calls, 1);
        assert!(backend.gate_held());
        assert!(backend.lease_present());
        assert_eq!(metadata.calls(), 0);
        assert!(fixture.journal().load().unwrap().is_none());
    }
}

#[test]
fn version_inode_or_pending_install_drift_requires_recovery_and_gate() {
    let base = base_inodes();
    let drifts = [
        observation(identity(10_321, SIGNATURE, 2), base, false),
        observation(
            identity(10_321, SIGNATURE, 1),
            DataInodes::new(102, 202).unwrap(),
            false,
        ),
        observation(identity(10_321, SIGNATURE, 1), base, true),
    ];
    for live in drifts {
        let fixture = Fixture::new();
        let mut backend = fixture.backend();
        backend.set_observation(live);
        let mut platform = fixture.platform(backend, FakeMetadata::default(), CrashOnce::never());

        assert_eq!(
            platform.rescue_to_base(&Fixture::key()),
            RescueExecution::RecoveryRequired
        );
        let (backend, metadata, _) = platform.into_dependencies();
        assert_eq!(backend.emergency_gate_calls, 1);
        assert!(backend.gate_held());
        assert!(backend.lease_present());
        assert_eq!(metadata.calls(), 0);
        assert!(fixture.journal().load().unwrap().is_none());
    }
}

#[test]
fn completed_journal_with_gate_drift_cannot_succeed_or_retire_lease() {
    let fixture = Fixture::new();
    let mut first = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::after_phase(RescuePhase::Completed),
    );
    assert_eq!(
        first.rescue_to_base(&Fixture::key()),
        RescueExecution::RecoveryRequired
    );
    let (mut backend, metadata, _) = first.into_dependencies();
    assert_eq!(backend.retire_calls, 0);
    assert!(backend.lease_present());
    backend.drift_gate(GateSnapshot::new(PackageEnabledState::Disabled, false));

    let mut resumed = fixture.platform(backend, metadata, CrashOnce::never());
    assert_eq!(
        resumed.rescue_to_base(&Fixture::key()),
        RescueExecution::RecoveryRequired
    );
    let (backend, _, _) = resumed.into_dependencies();
    assert_eq!(backend.retire_calls, 0);
    assert!(backend.lease_present());
    assert!(backend.gate_held());
    assert!(backend.native_base(fixture.managed()));
}

#[test]
fn failed_containment_is_never_reported_as_a_gated_recovery_state() {
    let fixture = Fixture::new();
    let mut backend = fixture.backend();
    let enrolled = fixture.managed();
    backend.set_observation(observation(
        identity(
            enrolled.identity().uid(),
            SIGNATURE,
            enrolled.identity().version_code() + 1,
        ),
        enrolled.base_inodes(),
        false,
    ));
    backend.reject_emergency_gate();
    let mut platform = fixture.platform(backend, FakeMetadata::default(), CrashOnce::never());

    assert_eq!(
        platform.rescue_to_base(&Fixture::key()),
        RescueExecution::ContainmentFailed
    );
}

fn observation(
    identity: uclone_slot_runtime::domain::AppIdentity,
    package_manager: DataInodes,
    pending_install: bool,
) -> PackageObservation {
    let visible = DataInodes::new(301, 401).unwrap();
    PackageObservation::new(identity, package_manager, visible, visible, pending_install)
}
