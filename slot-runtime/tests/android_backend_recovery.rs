#![doc = "Read-only reboot reconciliation coverage for the Android backend."]
#![allow(clippy::unwrap_used, reason = "validated test fixtures")]

use uclone_slot_runtime::android::{
    AndroidBackend, AndroidCommand, CanonicalView, CommandError, CommandRunner, MountCounts,
    MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageKey, PackageName,
    PackageObservation, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::reconcile::RecoveryBackend;
use uclone_slot_runtime::runtime::PlatformError;

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
struct RecoveryProbe {
    proof: ViewProof,
    unlocked: bool,
}

impl PackageProbe for RecoveryProbe {
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
        Err(ProbeError::Unavailable)
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
        Err(ProbeError::Unavailable)
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(self.unlocked)
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

#[test]
fn native_base_recovery_proof_is_read_only() {
    let package = managed();
    let proof = ViewProof::new(
        CanonicalView::new(package.base_inodes(), MountCounts::new(0, 0)),
        package.base_inodes(),
        package.base_inodes(),
    );
    let probe = RecoveryProbe {
        proof,
        unlocked: true,
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    assert!(backend.user0_unlocked().unwrap());
    backend.verify_native_base(&package).unwrap();

    assert!(backend.runner().commands.is_empty());
}

#[test]
fn native_base_recovery_rejects_a_remaining_bind_mount() {
    let package = managed();
    let proof = ViewProof::new(
        CanonicalView::new(package.base_inodes(), MountCounts::new(1, 1)),
        package.base_inodes(),
        package.base_inodes(),
    );
    let probe = RecoveryProbe {
        proof,
        unlocked: true,
    };
    let mut backend = AndroidBackend::new(RecordingRunner::default(), probe);

    let error = backend.verify_native_base(&package).unwrap_err();

    assert_eq!(
        error,
        PlatformError::VerifySlotView {
            detail: "ce_mount_count_mismatch".to_owned(),
        }
    );
    assert!(backend.runner().commands.is_empty());
}
