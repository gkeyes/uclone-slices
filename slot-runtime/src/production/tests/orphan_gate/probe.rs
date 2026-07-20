use crate::android::{
    CanonicalView, MountCounts, MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
use crate::catalog::{PathSecurityProof, SecurityProfileProof};
use crate::domain::{
    AppIdentity, DataInodes, GateSnapshot, PackageName, PackageObservation, SlotId, UserId,
};

const BASE_CE: u64 = 977_407;
const BASE_DE: u64 = 977_417;

#[derive(Debug, Clone)]
pub(super) struct NativeBaseProbe {
    observation: PackageObservation,
}

impl NativeBaseProbe {
    pub(super) fn new() -> Self {
        let base = base_inodes();
        Self {
            observation: PackageObservation::new(identity(), base, base, base, false),
        }
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
        Ok(true)
    }
}

pub(super) fn base_inodes() -> DataInodes {
    DataInodes::new(BASE_CE, BASE_DE).unwrap()
}

pub(super) fn identity() -> AppIdentity {
    AppIdentity::new(
        10_321,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        7,
        "/data/app/slot-preview/base.apk",
    )
    .unwrap()
}

pub(super) fn security_profile() -> SecurityProfileProof {
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
