use crate::android::{
    CanonicalView, MountCounts, MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
use crate::catalog::{PathSecurityProof, SecurityProfileProof};
use crate::domain::{
    AppIdentity, DataInodes, GateSnapshot, PackageCandidate, PackageCompatibility, PackageName,
    PackageObservation, SlotId, UserId,
};
use std::cell::Cell;
use std::rc::Rc;

const BASE_CE: u64 = 977_407;
const BASE_DE: u64 = 977_417;

#[derive(Debug, Clone)]
pub(in crate::production::tests) struct NativeBaseProbe {
    observation: PackageObservation,
    user_unlocked: bool,
    pending_after_capture: Option<Rc<Cell<bool>>>,
}

impl NativeBaseProbe {
    pub(in crate::production::tests) fn new() -> Self {
        let base = base_inodes();
        Self {
            observation: PackageObservation::new(identity(), base, base, base, false),
            user_unlocked: true,
            pending_after_capture: None,
        }
    }

    pub(in crate::production::tests) fn direct_boot() -> Self {
        let base = base_inodes();
        Self {
            observation: PackageObservation::with_compatibility(
                identity(),
                base,
                base,
                base,
                false,
                PackageCompatibility::new(false, false, true),
            ),
            user_unlocked: true,
            pending_after_capture: None,
        }
    }

    pub(in crate::production::tests) fn locked_direct_boot() -> Self {
        Self {
            user_unlocked: false,
            ..Self::direct_boot()
        }
    }

    pub(in crate::production::tests) fn pending_install() -> Self {
        let base = base_inodes();
        Self {
            observation: PackageObservation::new(identity(), base, base, base, true),
            user_unlocked: true,
            pending_after_capture: None,
        }
    }

    pub(in crate::production::tests) fn pending_after_capture(captured: Rc<Cell<bool>>) -> Self {
        Self {
            pending_after_capture: Some(captured),
            ..Self::new()
        }
    }

    pub(in crate::production::tests) const fn set_user_unlocked(&mut self, unlocked: bool) {
        self.user_unlocked = unlocked;
    }
}

impl PackageProbe for NativeBaseProbe {
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Ok(MountNamespaceProof::new(7, 7))
    }

    fn observe_package(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<PackageObservation, ProbeError> {
        Ok(self.observation.clone())
    }

    fn inspect_package(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<PackageCandidate, ProbeError> {
        let pending = self.observation.pending_install()
            || self
                .pending_after_capture
                .as_ref()
                .is_some_and(|captured| captured.get());
        Ok(PackageCandidate::new(
            self.observation.identity().clone(),
            self.observation.package_manager_inodes(),
            pending,
            self.observation.compatibility(),
        ))
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
        Ok(0)
    }

    fn view_proof(
        &mut self,
        _package: &PackageName,
        _user_id: UserId,
    ) -> Result<ViewProof, ProbeError> {
        let base = base_inodes();
        Ok(ViewProof::new(
            CanonicalView::new(base, MountCounts::new(0, 0)),
            base,
            base,
        ))
    }

    fn slot_inodes(
        &mut self,
        _package: &PackageName,
        _slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        Ok(None)
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(self.user_unlocked)
    }
}

pub(in crate::production::tests) fn base_inodes() -> DataInodes {
    DataInodes::new(BASE_CE, BASE_DE).unwrap()
}

pub(in crate::production::tests) fn identity() -> AppIdentity {
    AppIdentity::new(
        10_321,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        7,
        "/data/app/slot-preview/base.apk",
    )
    .unwrap()
}

pub(in crate::production::tests) fn security_profile() -> SecurityProfileProof {
    let path = PathSecurityProof::new(
        identity().uid(),
        identity().uid(),
        0o700,
        "u:object_r:app_data_file:s0:c1,c2",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )
    .unwrap();
    SecurityProfileProof::new(path.clone(), path)
}
