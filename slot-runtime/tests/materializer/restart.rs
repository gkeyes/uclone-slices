use uclone_slot_runtime::materializer::{
    ArtifactState, FaultPoint, MaterializationCoordinator, MaterializationError,
    ScriptedFaultInjector,
};

use super::support::{Call, FakeBackend, managed_base, preview};

#[test]
fn every_in_process_crash_boundary_cleans_staging_and_uncommitted_ready() {
    for point in FaultPoint::ALL {
        let mut backend = FakeBackend::healthy();
        let mut faults = ScriptedFaultInjector::once(point);
        let error = MaterializationCoordinator::new(&mut backend, &mut faults)
            .materialize(&managed_base(), &preview())
            .unwrap_err();
        assert!(matches!(error, MaterializationError::FaultInjected(actual) if actual == point));
        assert_eq!(
            backend.artifacts,
            ArtifactState::Absent,
            "fault at {point:?}"
        );
    }
}

#[test]
fn restart_cleanup_removes_partial_pair_only_while_gate_and_base_are_proven() {
    for state in [
        ArtifactState::StagingOnly,
        ArtifactState::ReadyOnly,
        ArtifactState::Both,
    ] {
        let mut backend = FakeBackend::healthy();
        backend.artifacts = state;
        let mut faults = ScriptedFaultInjector::never();
        MaterializationCoordinator::new(&mut backend, &mut faults)
            .cleanup_interrupted(&managed_base(), &preview())
            .unwrap();
        assert_eq!(backend.artifacts, ArtifactState::Absent);
        assert_eq!(backend.calls.first(), Some(&Call::Gate));
        assert_eq!(backend.calls.get(1), Some(&Call::Quiet));
        assert!(backend.calls.contains(&Call::Base));
    }
}

#[test]
fn cleanup_failure_is_explicit_and_never_reports_success() {
    let mut backend = FakeBackend::healthy();
    backend.fail_call = Some(Call::Copy(
        uclone_slot_runtime::materializer::DataDomain::De,
    ));
    backend.cleanup_fails = true;
    let mut faults = ScriptedFaultInjector::never();
    let error = MaterializationCoordinator::new(&mut backend, &mut faults)
        .materialize(&managed_base(), &preview())
        .unwrap_err();

    assert!(matches!(error, MaterializationError::CleanupFailed { .. }));
    assert_eq!(backend.artifacts, ArtifactState::StagingOnly);
    assert!(!backend.calls.contains(&Call::Publish));
}
