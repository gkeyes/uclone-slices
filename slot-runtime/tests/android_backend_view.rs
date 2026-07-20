#![doc = "Paired CE and DE mount coverage for the Android runtime backend."]
#![allow(
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "private view fixtures share validated helpers with the parent test module"
)]

use std::collections::VecDeque;

use uclone_slot_runtime::android::{
    AndroidBackend, AndroidCommand, CommandError, CommandKind, CommandRunner, DataDomain,
    MountCounts, MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
use uclone_slot_runtime::domain::{
    DataInodes, GateSnapshot, PackageName, PackageObservation, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::layout::RuntimeLayout;
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};

#[path = "android_backend_view/support.rs"]
mod view_support;

use view_support::{coherent, held, managed};

#[derive(Debug)]
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
struct ViewProbe {
    gates: VecDeque<GateSnapshot>,
    proofs: VecDeque<ViewProof>,
    slot_inodes: VecDeque<Option<DataInodes>>,
}

impl PackageProbe for ViewProbe {
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Ok(MountNamespaceProof::new(7, 7))
    }

    fn observe_package(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<PackageObservation, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn gate_snapshot(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<GateSnapshot, ProbeError> {
        self.gates.pop_front().ok_or(ProbeError::InvalidResponse)
    }

    fn running_process_count(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<u32, ProbeError> {
        Err(ProbeError::Unavailable)
    }

    fn view_proof(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<ViewProof, ProbeError> {
        self.proofs.pop_front().ok_or(ProbeError::InvalidResponse)
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        _slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        self.slot_inodes
            .pop_front()
            .ok_or(ProbeError::InvalidResponse)
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(true)
    }
}

#[test]
fn binds_derived_ce_and_de_paths_and_verifies_one_mount_each() {
    let package = managed(None);
    let target = SlotView::new(
        SlotId::parse("work").unwrap(),
        DataInodes::new(303, 404).unwrap(),
    );
    let probe = ViewProbe {
        gates: VecDeque::from([held(), held()]),
        proofs: VecDeque::from([
            coherent(package.base_inodes(), MountCounts::new(0, 0)),
            coherent(target.inodes(), MountCounts::new(1, 1)),
        ]),
        slot_inodes: VecDeque::from([Some(target.inodes())]),
    };
    let runner = RecordingRunner {
        commands: Vec::new(),
    };
    let mut backend = AndroidBackend::new(runner, probe);

    backend.apply_slot_view(&package, &target).unwrap();
    backend.verify_slot_view(&package, &target).unwrap();

    let commands = &backend.runner().commands;
    assert_eq!(commands.len(), 2);
    let [ce_command, de_command] = commands.as_slice() else {
        return;
    };
    assert_eq!(ce_command.kind(), CommandKind::Bind(DataDomain::Ce));
    assert_eq!(de_command.kind(), CommandKind::Bind(DataDomain::De));
    let expected = RuntimeLayout::slot_paths(package.package_name(), target.slot_id());
    let canonical = RuntimeLayout::slot_paths(package.package_name(), &SlotId::base());
    assert_eq!(ce_command.source(), Some(expected.ce()));
    assert_eq!(ce_command.target(), Some(canonical.ce()));
    assert_eq!(de_command.source(), Some(expected.de()));
    assert_eq!(de_command.target(), Some(canonical.de()));
}

#[test]
fn base_view_unmounts_both_domains_and_requires_zero_mounts() {
    let active = SlotView::new(
        SlotId::parse("work").unwrap(),
        DataInodes::new(303, 404).unwrap(),
    );
    let package = managed(Some(active.clone()));
    let base = SlotView::new(SlotId::base(), package.base_inodes());
    let probe = ViewProbe {
        gates: VecDeque::from([held(), held()]),
        proofs: VecDeque::from([
            coherent(active.inodes(), MountCounts::new(1, 1)),
            coherent(base.inodes(), MountCounts::new(0, 0)),
        ]),
        slot_inodes: VecDeque::from([Some(active.inodes())]),
    };
    let runner = RecordingRunner {
        commands: Vec::new(),
    };
    let mut backend = AndroidBackend::new(runner, probe);

    backend.apply_slot_view(&package, &base).unwrap();
    backend.verify_slot_view(&package, &base).unwrap();

    let kinds: Vec<CommandKind> = backend
        .runner()
        .commands
        .iter()
        .map(AndroidCommand::kind)
        .collect();
    assert_eq!(
        kinds,
        [
            CommandKind::Unmount(DataDomain::Ce),
            CommandKind::Unmount(DataDomain::De),
        ]
    );
}

#[test]
fn slot_to_slot_replaces_the_old_pair_before_binding_the_new_pair() {
    let active = SlotView::new(
        SlotId::parse("slot-a").unwrap(),
        DataInodes::new(303, 404).unwrap(),
    );
    let package = managed(Some(active.clone()));
    let target = SlotView::new(
        SlotId::parse("slot-b").unwrap(),
        DataInodes::new(505, 606).unwrap(),
    );
    let probe = ViewProbe {
        gates: VecDeque::from([held(), held()]),
        proofs: VecDeque::from([
            coherent(active.inodes(), MountCounts::new(1, 1)),
            coherent(target.inodes(), MountCounts::new(1, 1)),
        ]),
        slot_inodes: VecDeque::from([Some(target.inodes()), Some(active.inodes())]),
    };
    let runner = RecordingRunner {
        commands: Vec::new(),
    };
    let mut backend = AndroidBackend::new(runner, probe);

    backend.apply_slot_view(&package, &target).unwrap();
    backend.verify_slot_view(&package, &target).unwrap();

    let kinds: Vec<CommandKind> = backend
        .runner()
        .commands
        .iter()
        .map(AndroidCommand::kind)
        .collect();
    assert_eq!(
        kinds,
        [
            CommandKind::Unmount(DataDomain::Ce),
            CommandKind::Unmount(DataDomain::De),
            CommandKind::Bind(DataDomain::Ce),
            CommandKind::Bind(DataDomain::De),
        ]
    );
}

#[test]
fn unknown_coherent_current_view_is_rejected_without_mount_commands() {
    let package = managed(None);
    let target = SlotView::new(
        SlotId::parse("work").unwrap(),
        DataInodes::new(303, 404).unwrap(),
    );
    let unknown = DataInodes::new(505, 606).unwrap();
    let probe = ViewProbe {
        gates: VecDeque::from([held()]),
        proofs: VecDeque::from([coherent(unknown, MountCounts::new(1, 1))]),
        slot_inodes: VecDeque::new(),
    };
    let mut backend = AndroidBackend::new(
        RecordingRunner {
            commands: Vec::new(),
        },
        probe,
    );

    let error = backend.apply_slot_view(&package, &target).unwrap_err();

    assert_eq!(
        error,
        PlatformError::ApplySlotView {
            detail: "canonical_ce_inode_mismatch".to_owned(),
        }
    );
    assert!(backend.runner().commands.is_empty());
}
