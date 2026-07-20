#![doc = "Partial CE and DE command failure coverage for the Android runtime backend."]
#![allow(clippy::unwrap_used, reason = "validated test fixtures")]

use std::collections::VecDeque;

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
struct SecondCommandFails {
    commands: Vec<AndroidCommand>,
}

impl CommandRunner for SecondCommandFails {
    fn run(&mut self, command: &AndroidCommand) -> Result<(), CommandError> {
        self.commands.push(command.clone());
        if self.commands.len() == 2 {
            Err(CommandError::Rejected)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct ReadyProbe {
    proofs: VecDeque<ViewProof>,
    source: DataInodes,
}

impl PackageProbe for ReadyProbe {
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
        self.proofs.pop_front().ok_or(ProbeError::InvalidResponse)
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        _slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        Ok(Some(self.source))
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(true)
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
fn second_bind_failure_reports_partial_pair_and_never_success() {
    let package = managed();
    let target = SlotView::new(
        SlotId::parse("work").unwrap(),
        DataInodes::new(303, 404).unwrap(),
    );
    let base = package.base_inodes();
    let mixed = DataInodes::new(target.inodes().ce().get(), base.de().get()).unwrap();
    let partial = ViewProof::new(
        CanonicalView::new(mixed, MountCounts::new(1, 0)),
        mixed,
        mixed,
    );
    let restored = ViewProof::new(CanonicalView::new(base, MountCounts::new(0, 0)), base, base);
    let probe = ReadyProbe {
        proofs: VecDeque::from([restored, partial, restored]),
        source: target.inodes(),
    };
    let mut backend = AndroidBackend::new(SecondCommandFails::default(), probe);

    let error = backend.apply_slot_view(&package, &target).unwrap_err();

    assert_eq!(
        error,
        PlatformError::ApplySlotView {
            detail: "partial_bind_de_failed".to_owned(),
        }
    );
    let kinds: Vec<_> = backend
        .runner()
        .commands
        .iter()
        .map(AndroidCommand::kind)
        .collect();
    assert_eq!(
        kinds,
        [
            uclone_slot_runtime::android::CommandKind::Bind(
                uclone_slot_runtime::android::DataDomain::Ce,
            ),
            uclone_slot_runtime::android::CommandKind::Bind(
                uclone_slot_runtime::android::DataDomain::De,
            ),
            uclone_slot_runtime::android::CommandKind::Unmount(
                uclone_slot_runtime::android::DataDomain::Ce,
            ),
        ]
    );
}
