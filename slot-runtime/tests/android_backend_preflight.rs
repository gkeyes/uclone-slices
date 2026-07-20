#![doc = "Mutation preflight coverage for the Android runtime backend."]
#![allow(clippy::unwrap_used, reason = "validated test fixtures")]

use uclone_slot_runtime::android::{
    AndroidBackend, AndroidCommand, CanonicalView, CommandError, CommandRunner, MountCounts,
    MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, PackageObservation, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::runtime::{PlatformError, RuntimeBackend};

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
struct PreflightProbe {
    proof: ViewProof,
    source: Option<DataInodes>,
    namespace: MountNamespaceProof,
}

impl PackageProbe for PreflightProbe {
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
        Ok(GateSnapshot::new(PackageEnabledState::DisabledUser, false))
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
        Ok(self.proof)
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        _slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        Ok(self.source)
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(true)
    }

    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Ok(self.namespace)
    }
}

fn managed() -> ManagedPackage {
    let base = DataInodes::new(101, 202).unwrap();
    ManagedPackage::new(
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
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

fn target() -> SlotView {
    SlotView::new(
        SlotId::parse("work").unwrap(),
        DataInodes::new(303, 404).unwrap(),
    )
}

const fn proof(package: &ManagedPackage, counts: MountCounts) -> ViewProof {
    ViewProof::new(
        CanonicalView::new(package.base_inodes(), counts),
        package.base_inodes(),
        package.base_inodes(),
    )
}

#[test]
fn missing_slot_source_is_rejected_before_mount_mutation() {
    let package = managed();
    let probe = PreflightProbe {
        proof: proof(&package, MountCounts::new(0, 0)),
        source: None,
        namespace: MountNamespaceProof::new(7, 7),
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    let error = backend.apply_slot_view(&package, &target()).unwrap_err();

    assert_eq!(
        error,
        PlatformError::ApplySlotView {
            detail: "slot_source_missing".to_owned(),
        }
    );
    assert!(backend.runner().commands.is_empty());
}

#[test]
fn wrong_slot_source_inode_is_rejected_before_mount_mutation() {
    let package = managed();
    let probe = PreflightProbe {
        proof: proof(&package, MountCounts::new(0, 0)),
        source: Some(DataInodes::new(999, 404).unwrap()),
        namespace: MountNamespaceProof::new(7, 7),
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    let error = backend.apply_slot_view(&package, &target()).unwrap_err();

    assert_eq!(
        error,
        PlatformError::ApplySlotView {
            detail: "slot_source_ce_inode_mismatch".to_owned(),
        }
    );
    assert!(backend.runner().commands.is_empty());
}

#[test]
fn unpaired_existing_mounts_are_rejected_before_any_command() {
    let package = managed();
    let target = target();
    let probe = PreflightProbe {
        proof: proof(&package, MountCounts::new(1, 0)),
        source: Some(target.inodes()),
        namespace: MountNamespaceProof::new(7, 7),
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    let error = backend.apply_slot_view(&package, &target).unwrap_err();

    assert_eq!(
        error,
        PlatformError::ApplySlotView {
            detail: "unpaired_mount_state".to_owned(),
        }
    );
    assert!(backend.runner().commands.is_empty());
}

#[test]
fn arbitrary_root_shell_mount_namespace_is_rejected_before_any_command() {
    let package = managed();
    let target = target();
    let probe = PreflightProbe {
        proof: proof(&package, MountCounts::new(0, 0)),
        source: Some(target.inodes()),
        namespace: MountNamespaceProof::new(7, 9),
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    let error = backend.apply_slot_view(&package, &target).unwrap_err();

    assert_eq!(
        error,
        PlatformError::ApplySlotView {
            detail: "mount_namespace_not_global".to_owned(),
        }
    );
    assert!(backend.runner().commands.is_empty());
}
