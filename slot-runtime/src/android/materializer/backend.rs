use crate::catalog::SecurityProfileProof;
use crate::domain::ManagedPackage;
use crate::materializer::{
    ArtifactState, BackendFailure, BaseAnchor, DataDomain, DomainCopyProof, MaterializationBackend,
    MaterializationPaths, SlotMaterializationProof, TreeSafetyProof,
};

use super::command::MaterializerCommand;
use super::evidence::run_quiet;
use super::executor::MaterializerExecutor;
use super::fsops;
use super::helpers::{domain_security, probe_failure};
use super::inspection::{inspect_base, inspect_pair};
use super::limits::MaterializerLimits;
use super::policy;
use super::tree::{inspect_tree, sync_tree};
use crate::android::PackageProbe;

#[doc = "Production fixed-layout Android materialization backend."]
#[derive(Debug)]
pub struct AndroidMaterializer<E, P> {
    pub(super) executor: E,
    pub(super) probe: P,
    pub(super) limits: MaterializerLimits,
}

impl<E: MaterializerExecutor, P: PackageProbe> MaterializationBackend
    for AndroidMaterializer<E, P>
{
    fn verify_gate_held(&mut self, package: &ManagedPackage) -> Result<(), BackendFailure> {
        policy::ensure_supported(package)?;
        self.require_gate(package)
    }

    fn verify_processes_quiesced(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<(), BackendFailure> {
        policy::ensure_supported(package)?;
        self.require_gate(package)?;
        let count = self
            .probe
            .running_process_count(package.package_name(), package.user_id())
            .map_err(probe_failure)?;
        if count == 0 {
            Ok(())
        } else {
            Err(BackendFailure::new("processes_still_running"))
        }
    }

    fn capture_base_anchor(
        &mut self,
        package: &ManagedPackage,
    ) -> Result<BaseAnchor, BackendFailure> {
        policy::ensure_supported(package)?;
        inspect_base(
            &mut self.executor,
            policy::base(DataDomain::Ce),
            policy::base(DataDomain::De),
            self.limits,
        )
    }

    fn artifact_state(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<ArtifactState, BackendFailure> {
        Self::prepare_paths(paths, false)?;
        fsops::artifact_state_at(
            paths.staging_ce(),
            paths.staging_de(),
            paths.ready_ce(),
            paths.ready_de(),
        )
    }

    fn cleanup_artifacts(&mut self, paths: &MaterializationPaths) -> Result<(), BackendFailure> {
        Self::prepare_paths(paths, false)?;
        fsops::cleanup_at([
            paths.staging_ce(),
            paths.staging_de(),
            paths.ready_ce(),
            paths.ready_de(),
        ])
    }

    fn create_staging(&mut self, paths: &MaterializationPaths) -> Result<(), BackendFailure> {
        Self::prepare_paths(paths, true)?;
        fsops::create_staging_at(
            paths.staging_ce(),
            paths.staging_de(),
            paths.ready_ce(),
            paths.ready_de(),
        )
    }

    fn copy_domain(
        &mut self,
        package: &ManagedPackage,
        paths: &MaterializationPaths,
        domain: DataDomain,
    ) -> Result<DomainCopyProof, BackendFailure> {
        policy::ensure_supported(package)?;
        Self::prepare_paths(paths, false)?;
        let source = policy::base(domain);
        let target = policy::staging(paths, domain);
        let source_tree =
            inspect_tree(source, self.limits).map_err(|error| BackendFailure::new(error.code()))?;
        if source_tree.safety() != TreeSafetyProof::clean() {
            return Err(BackendFailure::new("base_tree_unsafe"));
        }
        fsops::ensure_real_directory(target, "staging_root_invalid")?;
        run_quiet(
            &mut self.executor,
            &MaterializerCommand::copy(domain, source, target),
            "copy_command_failed",
        )?;
        let copied =
            inspect_tree(target, self.limits).map_err(|error| BackendFailure::new(error.code()))?;
        DomainCopyProof::new(domain, copied.digest(), copied.safety())
            .map_err(|_| BackendFailure::new("copy_proof_invalid"))
    }

    fn apply_security(
        &mut self,
        package: &ManagedPackage,
        paths: &MaterializationPaths,
        expected: &SecurityProfileProof,
    ) -> Result<(), BackendFailure> {
        policy::ensure_supported(package)?;
        Self::prepare_paths(paths, false)?;
        for domain in [DataDomain::Ce, DataDomain::De] {
            let security = domain_security(expected, domain);
            if security.uid() != package.identity().uid() {
                return Err(BackendFailure::new("security_uid_mismatch"));
            }
            let target = policy::staging(paths, domain);
            let tree = inspect_tree(target, self.limits)
                .map_err(|error| BackendFailure::new(error.code()))?;
            if tree.safety() != TreeSafetyProof::clean() {
                return Err(BackendFailure::new("staging_tree_unsafe"));
            }
            self.apply_domain_security(domain, target, security)?;
        }
        Ok(())
    }

    fn inspect_staging(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure> {
        Self::prepare_paths(paths, false)?;
        inspect_pair(
            &mut self.executor,
            paths.staging_ce(),
            paths.staging_de(),
            self.limits,
        )
    }

    fn sync_domain(
        &mut self,
        paths: &MaterializationPaths,
        domain: DataDomain,
    ) -> Result<(), BackendFailure> {
        Self::prepare_paths(paths, false)?;
        sync_tree(policy::staging(paths, domain), self.limits)
            .map_err(|error| BackendFailure::new(error.code()))
    }

    fn publish_ready(
        &mut self,
        paths: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure> {
        Self::prepare_paths(paths, false)?;
        fsops::publish_at(
            paths.staging_ce(),
            paths.staging_de(),
            paths.ready_ce(),
            paths.ready_de(),
        )?;
        inspect_pair(
            &mut self.executor,
            paths.ready_ce(),
            paths.ready_de(),
            self.limits,
        )
    }
}
