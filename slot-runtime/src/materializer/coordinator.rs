use crate::domain::{ManagedPackage, SlotId};

use super::validation;
use super::{
    ArtifactState, BackendFailure, BaseAnchor, DataDomain, FaultInjector, FaultPoint,
    MaterializationBackend, MaterializationError, MaterializationPaths, MaterializationResult,
    MaterializationStage,
};

#[doc = "Paired CE/DE materialization coordinator that never restores the execution gate."]
#[derive(Debug)]
pub struct MaterializationCoordinator<'a, B, F> {
    backend: &'a mut B,
    faults: &'a mut F,
}

impl<'a, B, F> MaterializationCoordinator<'a, B, F>
where
    B: MaterializationBackend,
    F: FaultInjector,
{
    #[doc = "Constructs a coordinator over injected platform and fault boundaries."]
    pub const fn new(backend: &'a mut B, faults: &'a mut F) -> Self {
        Self { backend, faults }
    }

    #[doc = "Materializes the fixed `preview` slot while leaving the gate held."]
    pub fn materialize(
        &mut self,
        package: &ManagedPackage,
        slot: &SlotId,
    ) -> Result<MaterializationResult, MaterializationError> {
        validation::request(package, slot)?;
        let paths = MaterializationPaths::derive(package, slot);
        let base = self.preflight(package)?;
        self.fault(FaultPoint::BaseVerified)?;
        self.prepare_artifacts(&paths)?;
        let result = self.materialize_staging(package, slot, &paths, &base);
        match result {
            Ok(value) => Ok(value),
            Err(error) => Err(self.cleanup_after(&paths, error)),
        }
    }

    #[doc = "Deletes an explicitly uncommitted fixed Preview artifact after restart proofs."]
    pub fn cleanup_interrupted(
        &mut self,
        package: &ManagedPackage,
        slot: &SlotId,
    ) -> Result<(), MaterializationError> {
        validation::request(package, slot)?;
        let paths = MaterializationPaths::derive(package, slot);
        self.preflight(package)?;
        let state = self.artifact_state(&paths)?;
        if state == ArtifactState::Absent {
            return Ok(());
        }
        self.backend.cleanup_artifacts(&paths).map_err(|cleanup| {
            MaterializationError::CleanupFailed {
                original: "restart cleanup".to_owned(),
                cleanup,
            }
        })?;
        self.ensure_absent(&paths, "restart cleanup")
    }

    fn preflight(&mut self, package: &ManagedPackage) -> Result<BaseAnchor, MaterializationError> {
        self.backend
            .verify_gate_held(package)
            .map_err(|source| backend(MaterializationStage::Gate, source))?;
        self.backend
            .verify_processes_quiesced(package)
            .map_err(|source| backend(MaterializationStage::Quiesce, source))?;
        let base = self
            .backend
            .capture_base_anchor(package)
            .map_err(|source| backend(MaterializationStage::BaseAnchor, source))?;
        validation::initial_base(package, &base)?;
        Ok(base)
    }

    fn prepare_artifacts(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<(), MaterializationError> {
        match self.artifact_state(paths)? {
            ArtifactState::Absent => Ok(()),
            ArtifactState::ReadyOnly => Err(MaterializationError::ReadyAlreadyExists),
            ArtifactState::Both => Err(MaterializationError::AmbiguousArtifacts),
            ArtifactState::StagingOnly => {
                self.backend.cleanup_artifacts(paths).map_err(|cleanup| {
                    MaterializationError::CleanupFailed {
                        original: "stale staging cleanup".to_owned(),
                        cleanup,
                    }
                })?;
                self.ensure_absent(paths, "stale staging cleanup")
            }
        }
    }

    fn materialize_staging(
        &mut self,
        package: &ManagedPackage,
        slot: &SlotId,
        paths: &MaterializationPaths,
        base: &BaseAnchor,
    ) -> Result<MaterializationResult, MaterializationError> {
        self.backend
            .create_staging(paths)
            .map_err(|source| backend(MaterializationStage::CreateStaging, source))?;
        self.fault(FaultPoint::StagingCreated)?;
        self.copy(package, paths, base, DataDomain::Ce)?;
        self.fault(FaultPoint::CeCopied)?;
        self.copy(package, paths, base, DataDomain::De)?;
        self.fault(FaultPoint::DeCopied)?;
        self.backend
            .apply_security(package, paths, base.security())
            .map_err(|source| backend(MaterializationStage::ApplySecurity, source))?;
        self.fault(FaultPoint::SecurityApplied)?;
        let staging = self
            .backend
            .inspect_staging(paths)
            .map_err(|source| backend(MaterializationStage::InspectStaging, source))?;
        self.fault(FaultPoint::StagingInspected)?;
        self.sync(paths, DataDomain::Ce)?;
        self.fault(FaultPoint::CeSynced)?;
        self.sync(paths, DataDomain::De)?;
        self.fault(FaultPoint::DeSynced)?;
        let final_base = self
            .backend
            .capture_base_anchor(package)
            .map_err(|source| backend(MaterializationStage::BaseAnchor, source))?;
        validation::unchanged_base(base, &final_base)?;
        validation::staging(package, base, &staging)?;
        self.fault(FaultPoint::PairVerified)?;
        let published = self
            .backend
            .publish_ready(paths)
            .map_err(|source| backend(MaterializationStage::Publish, source))?;
        if published != staging {
            return Err(MaterializationError::PublicationMismatch);
        }
        self.fault(FaultPoint::ReadyPublished)?;
        Ok(MaterializationResult::from_proof(
            package,
            slot.clone(),
            &published,
            paths.clone(),
        ))
    }

    fn copy(
        &mut self,
        package: &ManagedPackage,
        paths: &MaterializationPaths,
        base: &BaseAnchor,
        domain: DataDomain,
    ) -> Result<(), MaterializationError> {
        let proof = self
            .backend
            .copy_domain(package, paths, domain)
            .map_err(|source| backend(MaterializationStage::Copy(domain), source))?;
        validation::copied_domain(base, domain, &proof)
    }

    fn sync(
        &mut self,
        paths: &MaterializationPaths,
        domain: DataDomain,
    ) -> Result<(), MaterializationError> {
        self.backend
            .sync_domain(paths, domain)
            .map_err(|source| backend(MaterializationStage::Sync(domain), source))
    }

    fn artifact_state(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<ArtifactState, MaterializationError> {
        self.backend
            .artifact_state(paths)
            .map_err(|source| backend(MaterializationStage::InspectArtifacts, source))
    }

    fn ensure_absent(
        &mut self,
        paths: &MaterializationPaths,
        operation: &str,
    ) -> Result<(), MaterializationError> {
        match self.artifact_state(paths) {
            Ok(ArtifactState::Absent) => Ok(()),
            Ok(_) => Err(MaterializationError::CleanupFailed {
                original: operation.to_owned(),
                cleanup: BackendFailure::new("artifacts_remain_after_cleanup"),
            }),
            Err(error) => Err(MaterializationError::CleanupFailed {
                original: operation.to_owned(),
                cleanup: BackendFailure::new(&error.to_string()),
            }),
        }
    }

    fn cleanup_after(
        &mut self,
        paths: &MaterializationPaths,
        original: MaterializationError,
    ) -> MaterializationError {
        let description = original.to_string();
        if let Err(cleanup) = self.backend.cleanup_artifacts(paths) {
            return MaterializationError::CleanupFailed {
                original: description,
                cleanup,
            };
        }
        match self.ensure_absent(paths, &description) {
            Ok(()) => original,
            Err(error) => error,
        }
    }

    fn fault(&mut self, point: FaultPoint) -> Result<(), MaterializationError> {
        if self.faults.should_fail(point) {
            Err(MaterializationError::FaultInjected(point))
        } else {
            Ok(())
        }
    }
}

const fn backend(stage: MaterializationStage, source: BackendFailure) -> MaterializationError {
    MaterializationError::Backend { stage, source }
}
