use super::super::orphan_gate::probe;
use crate::android::{
    CanonicalView, MountCounts, MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
use crate::domain::{
    DataInodes, GateSnapshot, PackageCandidate, PackageName, PackageObservation, SlotId, UserId,
};

#[derive(Debug, Clone)]
pub(super) struct MatrixProbe {
    observation: Result<PackageObservation, ProbeError>,
    gate: Result<GateSnapshot, ProbeError>,
}

impl MatrixProbe {
    pub(super) fn healthy(gate: GateSnapshot) -> Self {
        Self {
            observation: Ok(PackageObservation::new(
                probe::identity(),
                probe::base_inodes(),
                probe::base_inodes(),
                probe::base_inodes(),
                false,
            )),
            gate: Ok(gate),
        }
    }

    pub(super) fn for_view(inodes: DataInodes) -> Self {
        Self {
            observation: Ok(PackageObservation::new(
                probe::identity(),
                probe::base_inodes(),
                inodes,
                inodes,
                false,
            )),
            gate: Ok(GateSnapshot::new(
                crate::domain::PackageEnabledState::Default,
                false,
            )),
        }
    }

    pub(super) fn with_observation(observation: PackageObservation) -> Self {
        Self {
            observation: Ok(observation),
            gate: Ok(GateSnapshot::new(
                crate::domain::PackageEnabledState::Default,
                false,
            )),
        }
    }

    pub(super) fn observation_failure() -> Self {
        Self {
            observation: Err(ProbeError::Unavailable),
            gate: Ok(GateSnapshot::new(
                crate::domain::PackageEnabledState::Default,
                false,
            )),
        }
    }

    pub(super) fn gate_failure() -> Self {
        Self {
            observation: Ok(PackageObservation::new(
                probe::identity(),
                probe::base_inodes(),
                probe::base_inodes(),
                probe::base_inodes(),
                false,
            )),
            gate: Err(ProbeError::Unavailable),
        }
    }
}

impl PackageProbe for MatrixProbe {
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        Ok(MountNamespaceProof::new(7, 7))
    }

    fn inspect_package(
        &mut self,
        _: &PackageName,
        _: UserId,
    ) -> Result<PackageCandidate, ProbeError> {
        self.observation.clone().map(|value| {
            PackageCandidate::new(
                value.identity().clone(),
                value.package_manager_inodes(),
                value.pending_install(),
                value.compatibility(),
            )
        })
    }

    fn observe_package(
        &mut self,
        _: &PackageName,
        _: UserId,
    ) -> Result<PackageObservation, ProbeError> {
        self.observation.clone()
    }

    fn gate_snapshot(&mut self, _: &PackageName, _: UserId) -> Result<GateSnapshot, ProbeError> {
        self.gate
    }

    fn running_process_count(&mut self, _: &PackageName, _: UserId) -> Result<u32, ProbeError> {
        Ok(0)
    }

    fn view_proof(&mut self, _: &PackageName, _: UserId) -> Result<ViewProof, ProbeError> {
        self.observation.as_ref().map_or_else(
            |error| Err(*error),
            |value| {
                Ok(ViewProof::new(
                    CanonicalView::new(value.canonical_inodes(), MountCounts::new(1, 1)),
                    value.active_process_inodes(),
                    value.active_process_inodes(),
                ))
            },
        )
    }

    fn slot_inodes(
        &mut self,
        _: &PackageName,
        _: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        Ok(None)
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        Ok(true)
    }
}
