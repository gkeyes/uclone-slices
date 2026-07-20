#![doc = "Cross-namespace view proof coverage for the Android runtime backend."]
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

#[derive(Debug)]
struct NoopRunner;

impl CommandRunner for NoopRunner {
    fn run(&mut self, _command: &AndroidCommand) -> Result<(), CommandError> {
        Ok(())
    }
}

#[derive(Debug)]
struct ProofProbe {
    proof: ViewProof,
    namespace: MountNamespaceProof,
}

impl PackageProbe for ProofProbe {
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
        Err(ProbeError::Unavailable)
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

#[test]
fn mirror_inode_disagreement_rejects_the_view() {
    let package = managed();
    let target = target();
    let proof = ViewProof::new(
        CanonicalView::new(target.inodes(), MountCounts::new(1, 1)),
        DataInodes::new(999, 404).unwrap(),
        target.inodes(),
    );
    let mut backend = AndroidBackend::new(
        NoopRunner,
        ProofProbe {
            proof,
            namespace: MountNamespaceProof::new(7, 7),
        },
    );

    let error = backend.verify_slot_view(&package, &target).unwrap_err();

    assert_eq!(
        error,
        PlatformError::VerifySlotView {
            detail: "mirror_ce_inode_mismatch".to_owned(),
        }
    );
}

#[test]
fn zygote_inode_disagreement_rejects_the_view() {
    let package = managed();
    let target = target();
    let proof = ViewProof::new(
        CanonicalView::new(target.inodes(), MountCounts::new(1, 1)),
        target.inodes(),
        DataInodes::new(303, 999).unwrap(),
    );
    let mut backend = AndroidBackend::new(
        NoopRunner,
        ProofProbe {
            proof,
            namespace: MountNamespaceProof::new(7, 7),
        },
    );

    let error = backend.verify_slot_view(&package, &target).unwrap_err();

    assert_eq!(
        error,
        PlatformError::VerifySlotView {
            detail: "zygote_de_inode_mismatch".to_owned(),
        }
    );
}

#[test]
fn extra_canonical_mount_layer_rejects_the_view() {
    let package = managed();
    let target = target();
    let proof = ViewProof::new(
        CanonicalView::new(target.inodes(), MountCounts::new(2, 2)),
        target.inodes(),
        target.inodes(),
    );
    let mut backend = AndroidBackend::new(
        NoopRunner,
        ProofProbe {
            proof,
            namespace: MountNamespaceProof::new(7, 7),
        },
    );

    let error = backend.verify_slot_view(&package, &target).unwrap_err();

    assert_eq!(
        error,
        PlatformError::VerifySlotView {
            detail: "ce_mount_count_mismatch".to_owned(),
        }
    );
}

#[test]
fn verification_rejects_a_non_global_daemon_mount_namespace() {
    let package = managed();
    let target = target();
    let proof = ViewProof::new(
        CanonicalView::new(target.inodes(), MountCounts::new(1, 1)),
        target.inodes(),
        target.inodes(),
    );
    let probe = ProofProbe {
        proof,
        namespace: MountNamespaceProof::new(7, 9),
    };
    let mut backend = AndroidBackend::new(NoopRunner, probe);

    let error = backend.verify_slot_view(&package, &target).unwrap_err();

    assert_eq!(
        error,
        PlatformError::VerifySlotView {
            detail: "mount_namespace_not_global".to_owned(),
        }
    );
}
