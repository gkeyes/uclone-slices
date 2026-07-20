use std::collections::VecDeque;

use uclone_slot_runtime::catalog::{PathSecurityProof, SecurityProfileProof};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, ManagedPackage, PackageKey, PackageName, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::materializer::{
    ArtifactState, BackendFailure, BaseAnchor, ContentProof, DataBytes, DataDomain,
    DirectoryAnchor, DomainCopyProof, MaterializationBackend, MaterializationPaths,
    SlotMaterializationProof, TreeSafetyProof,
};

pub(super) const CE_DIGEST: &str =
    "1111111111111111111111111111111111111111111111111111111111111111";
pub(super) const DE_DIGEST: &str =
    "2222222222222222222222222222222222222222222222222222222222222222";
const CE_POLICY: &str = "3333333333333333333333333333333333333333333333333333333333333333";
const DE_POLICY: &str = "4444444444444444444444444444444444444444444444444444444444444444";
const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Call {
    Gate,
    Quiet,
    Base,
    Capacity,
    InspectArtifacts,
    Cleanup,
    Create,
    Copy(DataDomain),
    ApplySecurity,
    InspectStaging,
    Sync(DataDomain),
    Publish,
}

#[derive(Debug)]
pub(super) struct FakeBackend {
    pub(super) calls: Vec<Call>,
    pub(super) artifacts: ArtifactState,
    pub(super) fail_call: Option<Call>,
    pub(super) cleanup_fails: bool,
    pub(super) base_samples: VecDeque<BaseAnchor>,
    pub(super) ce_copy: DomainCopyProof,
    pub(super) de_copy: DomainCopyProof,
    pub(super) staging: SlotMaterializationProof,
    pub(super) published: SlotMaterializationProof,
}

impl FakeBackend {
    pub(super) fn healthy() -> Self {
        let base = base_anchor();
        let slot = slot_proof();
        Self {
            calls: Vec::new(),
            artifacts: ArtifactState::Absent,
            fail_call: None,
            cleanup_fails: false,
            base_samples: VecDeque::from([base.clone(), base]),
            ce_copy: DomainCopyProof::new(DataDomain::Ce, CE_DIGEST, TreeSafetyProof::clean())
                .unwrap(),
            de_copy: DomainCopyProof::new(DataDomain::De, DE_DIGEST, TreeSafetyProof::clean())
                .unwrap(),
            staging: slot.clone(),
            published: slot,
        }
    }

    fn record(&mut self, call: Call) -> Result<(), BackendFailure> {
        self.calls.push(call);
        if self.fail_call == Some(call) {
            Err(BackendFailure::new("injected_backend_failure"))
        } else {
            Ok(())
        }
    }
}

impl MaterializationBackend for FakeBackend {
    fn verify_gate_held(&mut self, _: &ManagedPackage) -> Result<(), BackendFailure> {
        self.record(Call::Gate)
    }

    fn verify_processes_quiesced(&mut self, _: &ManagedPackage) -> Result<(), BackendFailure> {
        self.record(Call::Quiet)
    }

    fn capture_base_anchor(&mut self, _: &ManagedPackage) -> Result<BaseAnchor, BackendFailure> {
        self.record(Call::Base)?;
        self.base_samples
            .pop_front()
            .ok_or_else(|| BackendFailure::new("missing_base_sample"))
    }

    fn verify_capacity(
        &mut self,
        _: &ManagedPackage,
        _: &BaseAnchor,
    ) -> Result<(), BackendFailure> {
        self.record(Call::Capacity)
    }

    fn artifact_state(
        &mut self,
        _: &MaterializationPaths,
    ) -> Result<ArtifactState, BackendFailure> {
        self.record(Call::InspectArtifacts)?;
        Ok(self.artifacts)
    }

    fn cleanup_artifacts(&mut self, _: &MaterializationPaths) -> Result<(), BackendFailure> {
        self.calls.push(Call::Cleanup);
        if self.cleanup_fails {
            return Err(BackendFailure::new("cleanup_failed"));
        }
        self.artifacts = ArtifactState::Absent;
        Ok(())
    }

    fn create_staging(&mut self, _: &MaterializationPaths) -> Result<(), BackendFailure> {
        self.record(Call::Create)?;
        self.artifacts = ArtifactState::StagingOnly;
        Ok(())
    }

    fn copy_domain(
        &mut self,
        _: &ManagedPackage,
        _: &MaterializationPaths,
        domain: DataDomain,
    ) -> Result<DomainCopyProof, BackendFailure> {
        self.record(Call::Copy(domain))?;
        Ok(match domain {
            DataDomain::Ce => self.ce_copy.clone(),
            DataDomain::De => self.de_copy.clone(),
        })
    }

    fn apply_security(
        &mut self,
        _: &ManagedPackage,
        _: &MaterializationPaths,
        _: &SecurityProfileProof,
    ) -> Result<(), BackendFailure> {
        self.record(Call::ApplySecurity)
    }

    fn inspect_staging(
        &mut self,
        _: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure> {
        self.record(Call::InspectStaging)?;
        Ok(self.staging.clone())
    }

    fn sync_domain(
        &mut self,
        _: &MaterializationPaths,
        domain: DataDomain,
    ) -> Result<(), BackendFailure> {
        self.record(Call::Sync(domain))
    }

    fn publish_ready(
        &mut self,
        _: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure> {
        self.record(Call::Publish)?;
        self.artifacts = ArtifactState::ReadyOnly;
        Ok(self.published.clone())
    }
}

pub(super) fn managed_base() -> ManagedPackage {
    let package = PackageName::parse("com.uclone.slotprobe").unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    ManagedPackage::new(
        PackageKey::new(package, UserId::PRIMARY),
        AppIdentity::new(10_321, SIGNATURE, 7, "/data/app/slotprobe/base.apk").unwrap(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap()
}

pub(super) fn preview() -> SlotId {
    SlotId::parse("preview").unwrap()
}

pub(super) fn security() -> SecurityProfileProof {
    SecurityProfileProof::new(path_security(CE_POLICY), path_security(DE_POLICY))
}

fn path_security(policy: &str) -> PathSecurityProof {
    PathSecurityProof::new(
        10_321,
        10_321,
        0o700,
        "u:object_r:app_data_file:s0:c1,c2",
        policy,
    )
    .unwrap()
}

pub(super) fn base_anchor() -> BaseAnchor {
    BaseAnchor::new(
        DataInodes::new(100, 200).unwrap(),
        DirectoryAnchor::new(11, CE_DIGEST).unwrap(),
        DirectoryAnchor::new(12, DE_DIGEST).unwrap(),
        security(),
        DataBytes::new(4_096, 1_024),
    )
    .unwrap()
}

pub(super) fn slot_proof() -> SlotMaterializationProof {
    SlotMaterializationProof::new(
        DataInodes::new(300, 400).unwrap(),
        DirectoryAnchor::new(11, CE_DIGEST).unwrap(),
        DirectoryAnchor::new(12, DE_DIGEST).unwrap(),
        ContentProof::new(CE_DIGEST, DE_DIGEST).unwrap(),
        security(),
        TreeSafetyProof::clean(),
        TreeSafetyProof::clean(),
        true,
    )
    .unwrap()
}
