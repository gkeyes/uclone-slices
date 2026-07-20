use std::collections::VecDeque;

use uclone_slot_runtime::android::{AndroidBackend, ProbeError};
use uclone_slot_runtime::domain::{GateSnapshot, PackageEnabledState};
use uclone_slot_runtime::runtime::RuntimeBackend;

use super::{FakeProbe, RecordingLeaseStore, RecordingRunner, managed, observation};

#[test]
fn acquire_gate_retries_transient_boot_probe_failures() {
    let package = managed("com.uclone.slotprobe");
    let original = GateSnapshot::new(PackageEnabledState::Default, false);
    let probe = FakeProbe {
        observation: observation(&package),
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([
            Ok(original),
            Err(ProbeError::Unavailable),
            Err(ProbeError::InvalidResponse),
            Ok(original),
        ]),
        process_counts: VecDeque::new(),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        probe,
        RecordingLeaseStore::default(),
    );

    backend.capture_gate_snapshot(&package).unwrap();
    backend.acquire_gate(&package).unwrap();

    assert_eq!(backend.runner().commands.len(), 1);
    assert!(backend.probe().gates.is_empty());
}

#[test]
fn package_observation_retries_transient_boot_samples() {
    let package = managed("com.uclone.slotprobe");
    let expected = observation(&package);
    let probe = FakeProbe {
        observation: expected.clone(),
        observation_errors: VecDeque::from([ProbeError::Unavailable, ProbeError::InvalidResponse]),
        gates: VecDeque::new(),
        process_counts: VecDeque::new(),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    let actual = backend.observe_package(&package).unwrap();

    assert_eq!(actual, expected);
    assert!(backend.probe().observation_errors.is_empty());
}
