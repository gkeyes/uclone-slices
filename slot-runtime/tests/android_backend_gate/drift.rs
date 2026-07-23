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

#[test]
fn same_version_reinstall_is_rejected_before_gate_release() {
    let package = managed("com.uclone.slotprobe");
    let reinstalled = PackageObservation::new(
        AppIdentity::new(
            package.identity().uid(),
            package.identity().signature_sha256(),
            package.identity().version_code(),
            "/data/app/reinstalled/base.apk",
        )
        .unwrap(),
        package.base_inodes(),
        package.active_inodes(),
        package.active_inodes(),
        false,
    );

    assert_release_rejected(&package, reinstalled, "final_package_metadata_changed");
}

#[test]
fn package_manager_inode_drift_is_rejected_before_gate_release() {
    let package = managed("com.uclone.slotprobe");
    let drifted = PackageObservation::new(
        package.identity().clone(),
        DataInodes::new(909, 202).unwrap(),
        package.active_inodes(),
        package.active_inodes(),
        false,
    );

    assert_release_rejected(&package, drifted, "final_base_inodes_changed");
}

#[test]
fn pending_install_is_rejected_before_gate_release() {
    let package = managed("com.uclone.slotprobe");
    let pending = PackageObservation::new(
        package.identity().clone(),
        package.base_inodes(),
        package.active_inodes(),
        package.active_inodes(),
        true,
    );

    assert_release_rejected(&package, pending, "final_pending_install");
}

fn assert_release_rejected(package: &ManagedPackage, observed: PackageObservation, detail: &str) {
    let original = GateSnapshot::new(PackageEnabledState::Default, false);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let probe = FakeProbe {
        observation: observed,
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([Ok(original), Ok(original), Ok(held)]),
        process_counts: VecDeque::new(),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        probe,
        RecordingLeaseStore::default(),
    );

    let captured = backend.capture_gate_snapshot(package).unwrap();
    backend.acquire_gate(package).unwrap();
    let error = backend.restore_gate(package, captured).unwrap_err();

    assert_eq!(
        error,
        PlatformError::RestoreGate {
            detail: detail.to_owned(),
        }
    );
    assert_eq!(
        backend
            .runner()
            .commands
            .iter()
            .map(AndroidCommand::kind)
            .collect::<Vec<_>>(),
        [CommandKind::DisableUser]
    );
}
