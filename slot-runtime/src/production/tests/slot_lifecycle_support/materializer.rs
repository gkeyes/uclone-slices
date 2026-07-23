use crate::domain::{ManagedPackage, SlotId};
use crate::materializer::{
    ArtifactState, BackendFailure, BaseAnchor, ContentProof, DataBytes, DataDomain,
    DirectoryAnchor, DomainCopyProof, MaterializationBackend, MaterializationPaths,
    SlotMaterializationProof, TreeSafetyProof,
};

use super::super::orphan_gate::probe;

const CE_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const DE_DIGEST: &str = "2222222222222222222222222222222222222222222222222222222222222222";

#[derive(Debug)]
pub(in crate::production::tests) struct TrackingMaterializer {
    fail_publish: bool,
    fail_cleanup: bool,
    artifacts: Vec<(SlotId, ArtifactState)>,
    inspected: Vec<SlotId>,
    cleaned: Vec<SlotId>,
}

impl TrackingMaterializer {
    pub(in crate::production::tests) const fn healthy() -> Self {
        Self {
            fail_publish: false,
            fail_cleanup: false,
            artifacts: Vec::new(),
            inspected: Vec::new(),
            cleaned: Vec::new(),
        }
    }

    pub(in crate::production::tests) fn publication_failure() -> Self {
        Self {
            fail_publish: true,
            ..Self::healthy()
        }
    }

    pub(in crate::production::tests) fn cleanup_failure(slot: SlotId) -> Self {
        Self {
            fail_cleanup: true,
            artifacts: vec![(slot, ArtifactState::ReadyOnly)],
            ..Self::healthy()
        }
    }

    pub(in crate::production::tests) fn ready_slots(slots: &[SlotId]) -> Self {
        Self {
            artifacts: slots
                .iter()
                .cloned()
                .map(|slot| (slot, ArtifactState::ReadyOnly))
                .collect(),
            ..Self::healthy()
        }
    }

    pub(in crate::production::tests) fn cleaned(&self) -> &[SlotId] {
        &self.cleaned
    }

    pub(in crate::production::tests) fn state(&self, slot: &SlotId) -> ArtifactState {
        self.artifacts
            .iter()
            .find(|(candidate, _)| candidate == slot)
            .map_or(ArtifactState::Absent, |(_, state)| *state)
    }

    fn set_state(&mut self, slot: &SlotId, state: ArtifactState) {
        if let Some((_, current)) = self
            .artifacts
            .iter_mut()
            .find(|(candidate, _)| candidate == slot)
        {
            *current = state;
        } else {
            self.artifacts.push((slot.clone(), state));
        }
    }
}

impl MaterializationBackend for TrackingMaterializer {
    fn verify_gate_held(&mut self, _: &ManagedPackage) -> Result<(), BackendFailure> {
        Ok(())
    }

    fn verify_processes_quiesced(&mut self, _: &ManagedPackage) -> Result<(), BackendFailure> {
        Ok(())
    }

    fn capture_base_anchor(&mut self, _: &ManagedPackage) -> Result<BaseAnchor, BackendFailure> {
        Ok(base_anchor())
    }

    fn verify_capacity(
        &mut self,
        _: &ManagedPackage,
        _: &BaseAnchor,
    ) -> Result<(), BackendFailure> {
        Ok(())
    }

    fn artifact_state(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<ArtifactState, BackendFailure> {
        self.inspected.push(paths.slot().clone());
        Ok(self.state(paths.slot()))
    }

    fn cleanup_artifacts(&mut self, paths: &MaterializationPaths) -> Result<(), BackendFailure> {
        self.cleaned.push(paths.slot().clone());
        if self.fail_cleanup {
            return Err(failure("cleanup"));
        }
        self.set_state(paths.slot(), ArtifactState::Absent);
        Ok(())
    }

    fn create_staging(&mut self, paths: &MaterializationPaths) -> Result<(), BackendFailure> {
        self.set_state(paths.slot(), ArtifactState::StagingOnly);
        Ok(())
    }

    fn copy_domain(
        &mut self,
        _: &ManagedPackage,
        _: &MaterializationPaths,
        domain: DataDomain,
    ) -> Result<DomainCopyProof, BackendFailure> {
        DomainCopyProof::new(domain, digest(domain), TreeSafetyProof::clean())
            .map_err(|_| failure("copy proof"))
    }

    fn apply_security(
        &mut self,
        _: &ManagedPackage,
        _: &MaterializationPaths,
        _: &crate::catalog::SecurityProfileProof,
    ) -> Result<(), BackendFailure> {
        Ok(())
    }

    fn inspect_staging(
        &mut self,
        _: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure> {
        Ok(slot_proof())
    }

    fn sync_domain(
        &mut self,
        _: &MaterializationPaths,
        _: DataDomain,
    ) -> Result<(), BackendFailure> {
        Ok(())
    }

    fn publish_ready(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure> {
        if self.fail_publish {
            return Err(failure("publish"));
        }
        self.set_state(paths.slot(), ArtifactState::ReadyOnly);
        Ok(slot_proof())
    }
}

fn base_anchor() -> BaseAnchor {
    BaseAnchor::new(
        probe::base_inodes(),
        DirectoryAnchor::new(11, CE_DIGEST).unwrap(),
        DirectoryAnchor::new(12, DE_DIGEST).unwrap(),
        probe::security_profile(),
        DataBytes::new(4_096, 1_024),
    )
    .unwrap()
}

fn slot_proof() -> SlotMaterializationProof {
    SlotMaterializationProof::new(
        crate::domain::DataInodes::new(1_200_001, 1_200_002).unwrap(),
        DirectoryAnchor::new(11, CE_DIGEST).unwrap(),
        DirectoryAnchor::new(12, DE_DIGEST).unwrap(),
        ContentProof::new(CE_DIGEST, DE_DIGEST).unwrap(),
        probe::security_profile(),
        TreeSafetyProof::clean(),
        TreeSafetyProof::clean(),
        true,
    )
    .unwrap()
}

const fn digest(domain: DataDomain) -> &'static str {
    match domain {
        DataDomain::Ce => CE_DIGEST,
        DataDomain::De => DE_DIGEST,
    }
}

fn failure(detail: &str) -> BackendFailure {
    BackendFailure::new(detail)
}
