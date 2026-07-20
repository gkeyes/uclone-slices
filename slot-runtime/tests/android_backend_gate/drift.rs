use super::*;

#[test]
fn gate_drift_before_disable_is_rejected_without_mutation() {
    let package = managed("com.uclone.slotprobe");
    let captured = GateSnapshot::new(PackageEnabledState::Default, false);
    let drifted = GateSnapshot::new(PackageEnabledState::Enabled, true);
    let probe = FakeProbe {
        observation: observation(&package),
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([Ok(captured), Ok(drifted)]),
        process_counts: VecDeque::from([Ok(0)]),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        probe,
        RecordingLeaseStore::default(),
    );

    backend.capture_gate_snapshot(&package).unwrap();
    let error = backend.acquire_gate(&package).unwrap_err();

    assert_eq!(
        error,
        PlatformError::AcquireGate {
            detail: "gate_state_changed_before_acquire".to_owned(),
        }
    );
    assert!(backend.runner().commands.is_empty());
    assert_eq!(backend.gate_lease_store().persisted.len(), 1);
}

#[test]
fn exact_already_held_gate_is_idempotent_without_another_disable() {
    let package = managed("com.uclone.slotprobe");
    let captured = GateSnapshot::new(PackageEnabledState::Enabled, true);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, true);
    let probe = FakeProbe {
        observation: observation(&package),
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([Ok(captured), Ok(held)]),
        process_counts: VecDeque::from([Ok(0)]),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        probe,
        RecordingLeaseStore::default(),
    );

    backend.capture_gate_snapshot(&package).unwrap();
    backend.acquire_gate(&package).unwrap();

    assert!(backend.runner().commands.is_empty());
    assert_eq!(backend.gate_lease_store().persisted.len(), 1);
}

#[test]
fn held_enabled_state_with_suspension_drift_is_not_exact() {
    let package = managed("com.uclone.slotprobe");
    let captured = GateSnapshot::new(PackageEnabledState::Default, false);
    let ambiguous = GateSnapshot::new(PackageEnabledState::DisabledUser, true);
    let probe = FakeProbe {
        observation: observation(&package),
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([Ok(captured), Ok(ambiguous)]),
        process_counts: VecDeque::from([Ok(0)]),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        probe,
        RecordingLeaseStore::default(),
    );

    backend.capture_gate_snapshot(&package).unwrap();
    let error = backend.acquire_gate(&package).unwrap_err();

    assert_eq!(
        error,
        PlatformError::AcquireGate {
            detail: "gate_state_changed_before_acquire".to_owned(),
        }
    );
    assert!(backend.runner().commands.is_empty());
}
