#![doc = "Command and probe coverage for the Android package execution gate."]
#![allow(
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "private gate fixture exposes one recording lease store to its parent test"
)]

use std::collections::VecDeque;

use uclone_slot_runtime::android::{
    AndroidBackend, AndroidCommand, CanonicalView, CommandError, CommandKind, CommandRunner,
    MountCounts, MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, PackageObservation, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};

#[path = "android_backend_gate/drift.rs"]
mod gate_drift;
#[path = "android_backend_gate/support.rs"]
mod gate_support;
#[path = "android_backend_gate/quiesce.rs"]
mod quiesce;
#[path = "android_backend_gate/startup_probe.rs"]
mod startup_probe;

use gate_support::RecordingLeaseStore;

#[derive(Debug, Default)]
struct RecordingRunner {
    commands: Vec<AndroidCommand>,
}

impl CommandRunner for RecordingRunner {
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError> {
        self.commands.push(command.clone());
        Ok(())
    }
}

#[derive(Debug)]
struct FakeProbe {
    observation: PackageObservation,
    observation_errors: VecDeque<ProbeError>,
    gates: VecDeque<Result<GateSnapshot, ProbeError>>,
    process_counts: VecDeque<Result<u32, ProbeError>>,
    probe_calls: usize,
}

impl PackageProbe for FakeProbe {
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Ok(MountNamespaceProof::new(7, 7))
    }

    fn observe_package(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<PackageObservation, ProbeError> {
        self.probe_calls += 1;
        if let Some(error) = self.observation_errors.pop_front() {
            return Err(error);
        }
        Ok(self.observation.clone())
    }

    fn gate_snapshot(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<GateSnapshot, ProbeError> {
        self.probe_calls += 1;
        self.gates
            .pop_front()
            .unwrap_or(Err(ProbeError::InvalidResponse))
    }

    fn running_process_count(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<u32, ProbeError> {
        self.probe_calls += 1;
        self.process_counts
            .pop_front()
            .unwrap_or(Err(ProbeError::InvalidResponse))
    }

    fn view_proof(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<ViewProof, ProbeError> {
        let inodes = self.observation.canonical_inodes();
        let mounts = u32::from(inodes != self.observation.package_manager_inodes());
        Ok(ViewProof::new(
            CanonicalView::new(inodes, MountCounts::new(mounts, mounts)),
            inodes,
            inodes,
        ))
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        _slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(true)
    }
}

fn managed(package: &str) -> ManagedPackage {
    let base = DataInodes::new(101, 202).unwrap();
    ManagedPackage::new(
        PackageKey::new(PackageName::parse(package).unwrap(), UserId::PRIMARY),
        AppIdentity::new(
            10_321,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            7,
            "/data/app/slotprobe/base.apk",
        )
        .unwrap(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap()
}

fn observation(package: &ManagedPackage) -> PackageObservation {
    PackageObservation::new(
        package.identity().clone(),
        package.base_inodes(),
        package.active_inodes(),
        package.active_inodes(),
        false,
    )
}

#[test]
fn exact_gate_sequence_when_probe_confirms_each_state() {
    let package = managed("com.uclone.slotprobe");
    let original = GateSnapshot::new(PackageEnabledState::Default, false);
    let held = GateSnapshot::new(PackageEnabledState::DisabledUser, false);
    let probe = FakeProbe {
        observation: observation(&package),
        observation_errors: VecDeque::new(),
        gates: VecDeque::from([
            Ok(original),
            Ok(original),
            Ok(held),
            Ok(held),
            Ok(held),
            Ok(held),
            Ok(original),
            Ok(original),
        ]),
        process_counts: VecDeque::from([Ok(0)]),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::with_gate_lease_store(
        RecordingRunner::default(),
        probe,
        RecordingLeaseStore::default(),
    );

    let captured = backend.capture_gate_snapshot(&package).unwrap();
    backend.acquire_gate(&package).unwrap();
    backend.verify_gate_held(&package).unwrap();
    backend.quiesce_processes(&package).unwrap();
    backend.restore_gate(&package, captured).unwrap();
    assert!(backend.gate_lease_store().removed.is_empty());
    backend.retire_gate_lease(&package).unwrap();

    let kinds: Vec<CommandKind> = backend
        .runner()
        .commands
        .iter()
        .map(AndroidCommand::kind)
        .collect();
    assert_eq!(
        kinds,
        [
            CommandKind::DisableUser,
            CommandKind::ForceStop,
            CommandKind::RestoreSuspended(false),
            CommandKind::RestoreEnabled(PackageEnabledState::Default),
        ]
    );
    let lease = backend.gate_lease_store().persisted.first().unwrap();
    assert_eq!(lease.package_name(), package.package_name());
    assert_eq!(lease.user_id(), UserId::PRIMARY);
    assert_eq!(lease.snapshot(), original);
    assert_eq!(lease.base_inodes(), package.base_inodes());
    assert_eq!(
        backend.gate_lease_store().removed,
        [package.package_name().clone()]
    );
}

#[test]
fn observation_is_returned_only_by_the_injected_probe() {
    let package = managed("com.uclone.slotprobe");
    let expected = observation(&package);
    let probe = FakeProbe {
        observation: expected.clone(),
        observation_errors: VecDeque::new(),
        gates: VecDeque::new(),
        process_counts: VecDeque::from([Ok(0)]),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    let actual = backend.observe_package(&package).unwrap();

    assert_eq!(actual, expected);
    assert!(backend.runner().commands.is_empty());
}

#[test]
fn any_valid_package_is_observed_through_the_injected_probe() {
    let package = managed("com.example.other");
    let expected = observation(&package);
    let probe = FakeProbe {
        observation: expected.clone(),
        observation_errors: VecDeque::new(),
        gates: VecDeque::new(),
        process_counts: VecDeque::from([Ok(0)]),
        probe_calls: 0,
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    let actual = backend.observe_package(&package).unwrap();

    assert_eq!(actual, expected);
    assert!(backend.runner().commands.is_empty());
    assert_eq!(backend.probe().probe_calls, 1);
}

#[test]
fn canonical_view_fixture_has_typed_mount_counts() {
    let base = DataInodes::new(101, 202).unwrap();
    let view = CanonicalView::new(base, MountCounts::new(0, 0));
    assert_eq!(view.inodes(), base);
    assert_eq!(view.mount_counts(), MountCounts::new(0, 0));
}
