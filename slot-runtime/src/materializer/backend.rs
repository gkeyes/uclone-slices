use crate::catalog::SecurityProfileProof;
use crate::domain::ManagedPackage;

use super::{
    ArtifactState, BackendFailure, BaseAnchor, DataDomain, DomainCopyProof, MaterializationPaths,
    SlotMaterializationProof,
};

#[doc = "Injected Android boundary for paired CE/DE slot materialization."]
pub trait MaterializationBackend: core::fmt::Debug {
    #[doc = "Proves the application execution gate is already held."]
    fn verify_gate_held(&mut self, package: &ManagedPackage) -> Result<(), BackendFailure>;

    #[doc = "Proves no process belonging to the managed package remains alive."]
    fn verify_processes_quiesced(&mut self, package: &ManagedPackage)
    -> Result<(), BackendFailure>;

    #[doc = "Captures immutable inode, content, security, and filesystem anchors for base."]
    fn capture_base_anchor(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<BaseAnchor, BackendFailure>;

    #[doc = "Proves clone materialization has bounded free space before creating staging."]
    fn verify_capacity(
        &mut self,
        package: &ManagedPackage,
        base: &BaseAnchor,
    ) -> Result<(), BackendFailure>;

    #[doc = "Inspects only the coordinator-derived staging and ready paths."]
    fn artifact_state(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<ArtifactState, BackendFailure>;

    #[doc = "Removes coordinator-derived staging and ready artifacts without touching base."]
    fn cleanup_artifacts(&mut self, paths: &MaterializationPaths) -> Result<(), BackendFailure>;

    #[doc = "Creates paired empty CE and DE staging directories."]
    fn create_staging(&mut self, paths: &MaterializationPaths) -> Result<(), BackendFailure>;

    #[doc = "Copies one base data domain into its fixed staging directory."]
    fn copy_domain(
        &mut self,
        package: &ManagedPackage,
        paths: &MaterializationPaths,
        domain: DataDomain,
    ) -> Result<DomainCopyProof, BackendFailure>;

    #[doc = "Applies the complete base security profile to both staging domains."]
    fn apply_security(
        &mut self,
        package: &ManagedPackage,
        paths: &MaterializationPaths,
        expected: &SecurityProfileProof,
    ) -> Result<(), BackendFailure>;

    #[doc = "Reads staging inode, content, tree, security, MCS, and fscrypt evidence."]
    fn inspect_staging(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure>;

    #[doc = "Synchronizes one complete staging data domain and its directory metadata."]
    fn sync_domain(
        &mut self,
        paths: &MaterializationPaths,
        domain: DataDomain,
    ) -> Result<(), BackendFailure>;

    #[doc = "Atomically publishes the verified pair and returns final ready-path proof."]
    fn publish_ready(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure>;
}
