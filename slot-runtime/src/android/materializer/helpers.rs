use std::path::Path;

use crate::android::{PackageProbe, ProbeError};
use crate::catalog::{PathSecurityProof, SecurityProfileProof};
use crate::domain::{ManagedPackage, PackageEnabledState};
use crate::materializer::{BackendFailure, DataDomain, MaterializationPaths};

use super::backend::AndroidMaterializer;
use super::command::MaterializerCommand;
use super::evidence::run_quiet;
use super::executor::MaterializerExecutor;
use super::fsops;
use super::limits::MaterializerLimits;
use super::policy;

impl<E, P> AndroidMaterializer<E, P> {
    #[doc = "Creates an adapter with fixed production traversal limits."]
    pub const fn new(executor: E, probe: P) -> Self {
        Self {
            executor,
            probe,
            limits: MaterializerLimits::PRODUCTION,
        }
    }

    #[doc = "Creates an adapter with equal-or-stricter validated traversal limits."]
    pub const fn with_limits(executor: E, probe: P, limits: MaterializerLimits) -> Self {
        Self {
            executor,
            probe,
            limits,
        }
    }

    #[doc = "Returns the injected typed command executor."]
    pub const fn executor(&self) -> &E {
        &self.executor
    }

    #[doc = "Returns the injected read-only Android probe."]
    pub const fn probe(&self) -> &P {
        &self.probe
    }
}

impl<E: MaterializerExecutor, P: PackageProbe> AndroidMaterializer<E, P> {
    pub(super) fn require_gate(&mut self, package: &ManagedPackage) -> Result<(), BackendFailure> {
        let gate = self
            .probe
            .gate_snapshot(package.package_name(), package.user_id())
            .map_err(probe_failure)?;
        if gate.enabled_state() == PackageEnabledState::DisabledUser {
            Ok(())
        } else {
            Err(BackendFailure::new("gate_not_held"))
        }
    }

    pub(super) fn prepare_paths(
        paths: &MaterializationPaths,
        create: bool,
    ) -> Result<(), BackendFailure> {
        policy::validate_paths(paths)?;
        for domain in [DataDomain::Ce, DataDomain::De] {
            fsops::ensure_storage_parent(
                policy::anchor(domain),
                policy::parent(paths, domain),
                create,
            )?;
        }
        Ok(())
    }

    pub(super) fn apply_domain_security(
        &mut self,
        domain: DataDomain,
        target: &Path,
        security: &PathSecurityProof,
    ) -> Result<(), BackendFailure> {
        let commands = [
            MaterializerCommand::chown(domain, security.uid(), security.gid(), target),
            MaterializerCommand::chmod(domain, security.mode(), target),
            MaterializerCommand::chcon(domain, security.selinux_context(), target),
        ];
        for command in commands {
            run_quiet(&mut self.executor, &command, "security_command_failed")?;
        }
        Ok(())
    }
}

pub(super) const fn domain_security(
    security: &SecurityProfileProof,
    domain: DataDomain,
) -> &PathSecurityProof {
    match domain {
        DataDomain::Ce => security.ce(),
        DataDomain::De => security.de(),
    }
}

pub(super) fn probe_failure(error: ProbeError) -> BackendFailure {
    match error {
        ProbeError::Unavailable => BackendFailure::new("probe_unavailable"),
        ProbeError::InvalidResponse => BackendFailure::new("probe_invalid_response"),
    }
}
