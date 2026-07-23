use uclone_slot_runtime::domain::{GateSnapshot, PackageEnabledState};
use uclone_slot_runtime::rescue::{RescueExecution, RescuePhase, RescueStatus};

use super::support::{CrashOnce, FakeMetadata, Fixture};

#[test]
fn fresh_rescue_retires_preview_to_native_base_and_exact_gate() {
    let fixture = Fixture::new();
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );

    assert_eq!(
        platform.rescue_to_base(&Fixture::key()),
        RescueExecution::CompletedBase
    );

    let (backend, metadata, _) = platform.into_dependencies();
    assert!(backend.native_base(fixture.managed()));
    assert_eq!(backend.native_restore_calls, 1);
    assert_eq!(backend.gate_restore_calls, 1);
    assert_eq!(backend.retire_calls, 1);
    assert!(!backend.lease_present());
    assert!(!backend.gate_held());
    assert_eq!(
        backend.gate_current(),
        GateSnapshot::new(PackageEnabledState::Enabled, true)
    );
    assert_eq!(metadata.calls(), 1);
    assert_eq!(
        fixture.journal().load().unwrap().unwrap().status(),
        RescueStatus::BaseRetired
    );
}

#[test]
fn every_durable_phase_resumes_after_process_restart() {
    let phases = [
        RescuePhase::Prepared,
        RescuePhase::GateHeld,
        RescuePhase::ProcessesQuiesced,
        RescuePhase::BaseApplying,
        RescuePhase::BaseVerified,
        RescuePhase::BaseCommitted,
        RescuePhase::GateReleased,
        RescuePhase::Completed,
    ];
    for phase in phases {
        let fixture = Fixture::new();
        let mut first = fixture.platform(
            fixture.backend(),
            FakeMetadata::default(),
            CrashOnce::after_phase(phase),
        );
        assert_eq!(
            first.rescue_to_base(&Fixture::key()),
            RescueExecution::RecoveryRequired,
            "injected boundary {phase:?}"
        );
        assert_eq!(
            fixture.journal().load().unwrap().unwrap().phase(),
            phase,
            "durable boundary {phase:?}"
        );

        let (backend, metadata, _) = first.into_dependencies();
        let mut resumed = fixture.platform(backend, metadata, CrashOnce::never());
        assert_eq!(
            resumed.rescue_to_base(&Fixture::key()),
            RescueExecution::CompletedBase,
            "restart from {phase:?}"
        );
        let (backend, metadata, _) = resumed.into_dependencies();
        assert!(backend.native_base(fixture.managed()));
        assert!(!backend.lease_present());
        assert_eq!(metadata.calls(), 1, "no second rescue epoch at {phase:?}");
        assert_eq!(
            fixture.journal().load().unwrap().unwrap().status(),
            RescueStatus::BaseRetired
        );
    }
}

#[test]
fn crash_after_gate_restore_completes_idempotently_without_second_restore() {
    let fixture = Fixture::new();
    let mut first = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::after_gate_restore(),
    );
    assert_eq!(
        first.rescue_to_base(&Fixture::key()),
        RescueExecution::RecoveryRequired
    );
    let (backend, metadata, _) = first.into_dependencies();
    assert_eq!(
        fixture.journal().load().unwrap().unwrap().phase(),
        RescuePhase::BaseCommitted
    );
    assert_eq!(backend.gate_restore_calls, 1);
    assert!(!backend.gate_held());
    assert!(backend.lease_present());

    let mut resumed = fixture.platform(backend, metadata, CrashOnce::never());
    assert_eq!(
        resumed.rescue_to_base(&Fixture::key()),
        RescueExecution::CompletedBase
    );
    let (backend, _, _) = resumed.into_dependencies();
    assert_eq!(backend.gate_restore_calls, 1);
    assert_eq!(backend.retire_calls, 1);
    assert!(!backend.lease_present());
}

#[test]
fn failed_gate_lease_retirement_keeps_lease_and_requires_recovery() {
    let fixture = Fixture::new();
    let mut backend = fixture.backend();
    backend.reject_retire_gate_lease();
    let mut platform = fixture.platform(backend, FakeMetadata::default(), CrashOnce::never());

    assert_eq!(
        platform.rescue_to_base(&Fixture::key()),
        RescueExecution::RecoveryRequired
    );

    let (backend, _, _) = platform.into_dependencies();
    assert!(backend.native_base(fixture.managed()));
    assert_eq!(backend.retire_calls, 1);
    assert!(backend.lease_present());
    assert!(backend.gate_held());
}

#[test]
fn repeated_retirement_failures_do_not_exhaust_rescue_steps() {
    let fixture = Fixture::new();
    let mut backend = fixture.backend();
    backend.reject_retire_gate_lease();
    let mut platform = fixture.platform(backend, FakeMetadata::default(), CrashOnce::never());

    for _ in 0..100 {
        assert_eq!(
            platform.rescue_to_base(&Fixture::key()),
            RescueExecution::RecoveryRequired
        );
    }
    let (mut backend, metadata, _) = platform.into_dependencies();
    let transaction = fixture.journal().load().unwrap().unwrap();
    assert_eq!(transaction.phase(), RescuePhase::Completed);
    assert_eq!(transaction.steps().len(), 8);

    backend.allow_retire_gate_lease();
    let mut resumed = fixture.platform(backend, metadata, CrashOnce::never());
    assert_eq!(
        resumed.rescue_to_base(&Fixture::key()),
        RescueExecution::CompletedBase
    );
    assert_eq!(fixture.journal().load().unwrap().unwrap().steps().len(), 8);
}
