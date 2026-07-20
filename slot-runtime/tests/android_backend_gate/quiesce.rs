use std::collections::VecDeque;

use uclone_slot_runtime::android::{AndroidBackend, ProbeError};
use uclone_slot_runtime::domain::{GateSnapshot, PackageEnabledState};
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};

use super::{FakeProbe, RecordingRunner, managed, observation};

const fn held_gate() -> GateSnapshot {
    GateSnapshot::new(PackageEnabledState::DisabledUser, false)
}

#[test]
fn quiesce_waits_for_a_process_that_is_finishing_exit() {
    let package = managed("com.uclone.slotprobe");
    let probe = FakeProbe {
        observation: observation(&package),
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([Ok(held_gate())]),
        process_counts: VecDeque::from([Ok(1), Ok(0)]),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    backend.quiesce_processes(&package).unwrap();

    assert_eq!(backend.runner().commands.len(), 1);
    assert_eq!(backend.probe().process_counts.len(), 0);
}

#[test]
fn quiesce_retries_a_transitional_invalid_process_sample() {
    let package = managed("com.uclone.slotprobe");
    let probe = FakeProbe {
        observation: observation(&package),
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([Ok(held_gate())]),
        process_counts: VecDeque::from([Err(ProbeError::InvalidResponse), Ok(0)]),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    backend.quiesce_processes(&package).unwrap();

    assert_eq!(backend.runner().commands.len(), 1);
    assert_eq!(backend.probe().process_counts.len(), 0);
}

#[test]
fn quiesce_retries_boot_time_process_probe_unavailability() {
    let package = managed("com.uclone.slotprobe");
    let probe = FakeProbe {
        observation: observation(&package),
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([Ok(held_gate())]),
        process_counts: VecDeque::from([
            Err(ProbeError::Unavailable),
            Err(ProbeError::InvalidResponse),
            Ok(0),
        ]),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    backend.quiesce_processes(&package).unwrap();

    assert_eq!(backend.runner().commands.len(), 1);
    assert!(backend.probe().process_counts.is_empty());
}

#[test]
fn quiesce_fails_closed_when_a_process_never_exits() {
    let package = managed("com.uclone.slotprobe");
    let probe = FakeProbe {
        observation: observation(&package),
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([Ok(held_gate())]),
        process_counts: std::iter::repeat_n(Ok(1), 401).collect(),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    let error = backend.quiesce_processes(&package).unwrap_err();

    assert_eq!(
        error,
        PlatformError::QuiesceProcesses {
            detail: "processes_still_running".to_owned(),
        }
    );
    assert_eq!(backend.runner().commands.len(), 1);
}
