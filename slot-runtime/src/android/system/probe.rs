#![allow(unreachable_pub, dead_code, unused_imports)]

use std::path::Path;

use crate::android::{CanonicalView, MountNamespaceProof, PackageProbe, ProbeError, ViewProof};
use crate::bridge::{
    ALLOWED_USER_ID, AppProcessRunner, BridgeClient, BridgeCommandRunner, PackageSnapshot,
};
use crate::domain::{
    AppIdentity, DataInodes, GateSnapshot, PackageCandidate, PackageCompatibility, PackageName,
    PackageObservation, SlotId, UserId,
};
use crate::layout::RuntimeLayout;

use super::executor::{ProcessExecutor, StdProcessExecutor};
use super::facts::{FactError, StdSystemFacts, SystemFacts};
use super::namespace::namespace_consensus;
use super::probe_mapping::{candidate_from_snapshot, map_bridge_error, map_fact_error};

/// Production read-only package probe backed by the typed bridge and fixed OS facts.
#[derive(Debug)]
pub struct SystemPackageProbe<R = AppProcessRunner, E = StdProcessExecutor, F = StdSystemFacts> {
    bridge: BridgeClient<R>,
    executor: E,
    facts: F,
    cached_package: Option<PackageSnapshot>,
}

impl SystemPackageProbe<AppProcessRunner, StdProcessExecutor, StdSystemFacts> {
    /// Constructs the production bridge, process executor, and standard filesystem reader.
    pub const fn new() -> Self {
        Self::with_dependencies(
            AppProcessRunner::new(),
            StdProcessExecutor::new(),
            StdSystemFacts::new(),
        )
    }
}

impl Default for SystemPackageProbe<AppProcessRunner, StdProcessExecutor, StdSystemFacts> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R: BridgeCommandRunner, E, F> SystemPackageProbe<R, E, F> {
    /// Constructs a probe from injected typed bridge, process, and read-only fact boundaries.
    pub const fn with_dependencies(bridge_runner: R, executor: E, facts: F) -> Self {
        Self {
            bridge: BridgeClient::new(bridge_runner),
            executor,
            facts,
            cached_package: None,
        }
    }

    /// Returns the injected process executor for deterministic inspection.
    pub const fn executor(&self) -> &E {
        &self.executor
    }

    /// Returns the injected read-only fact source for deterministic inspection.
    pub const fn facts(&self) -> &F {
        &self.facts
    }

    /// Returns the owned bridge runner, process executor, and fact source.
    pub fn into_dependencies(self) -> (R, E, F) {
        (self.bridge.into_inner(), self.executor, self.facts)
    }
}

impl<R: BridgeCommandRunner, E: ProcessExecutor, F: SystemFacts> PackageProbe
    for SystemPackageProbe<R, E, F>
{
    fn mount_namespace_proof(&mut self) -> Result<MountNamespaceProof, ProbeError> {
        let (daemon, init) = self.facts.mount_namespace_ids().map_err(map_fact_error)?;
        Ok(MountNamespaceProof::new(daemon, init))
    }

    fn inspect_package(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<PackageCandidate, ProbeError> {
        let snapshot = self.refresh_package_snapshot(package, user_id)?;
        candidate_from_snapshot(&snapshot)
    }

    fn observe_package(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<PackageObservation, ProbeError> {
        let snapshot = self.refresh_package_snapshot(package, user_id)?;
        let canonical = self
            .facts
            .canonical_inodes(package)
            .map_err(map_fact_error)?;
        let processes = self
            .facts
            .package_processes(package, snapshot.uid())
            .map_err(map_fact_error)?;
        let active = if processes.pids().is_empty() {
            let zygotes = self
                .facts
                .arm64_zygote_processes()
                .map_err(map_fact_error)?;
            namespace_consensus(&mut self.executor, package, &zygotes)?
        } else {
            namespace_consensus(&mut self.executor, package, &processes)?
        };
        let identity = AppIdentity::new(
            snapshot.uid(),
            snapshot.signature_sha256(),
            snapshot.version_code(),
            snapshot.code_path(),
        )
        .map_err(|_| ProbeError::InvalidResponse)?;
        let package_manager = DataInodes::new(
            snapshot.package_manager_ce_inode(),
            snapshot.package_manager_de_inode(),
        )
        .map_err(|_| ProbeError::InvalidResponse)?;
        Ok(PackageObservation::with_compatibility(
            identity,
            package_manager,
            canonical,
            active,
            snapshot.pending_install(),
            PackageCompatibility::new(
                snapshot.system_app(),
                snapshot.shared_uid(),
                snapshot.direct_boot_aware(),
            ),
        ))
    }

    fn gate_snapshot(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<GateSnapshot, ProbeError> {
        require_target(package, user_id)?;
        self.bridge
            .query_gate(package.as_str(), ALLOWED_USER_ID)
            .map_err(|error| map_bridge_error(&error))
    }

    fn running_process_count(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<u32, ProbeError> {
        require_target(package, user_id)?;
        let snapshot = match self.cached_package.as_ref() {
            Some(snapshot) => snapshot.clone(),
            None => self.refresh_package_snapshot(package, user_id)?,
        };
        let processes = self
            .facts
            .package_processes(package, snapshot.uid())
            .map_err(map_fact_error)?;
        u32::try_from(processes.pids().len()).map_err(|_| ProbeError::InvalidResponse)
    }

    fn view_proof(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<ViewProof, ProbeError> {
        require_target(package, user_id)?;
        let canonical = self
            .facts
            .canonical_inodes(package)
            .map_err(map_fact_error)?;
        let mirror = self.facts.mirror_inodes(package).map_err(map_fact_error)?;
        let counts = self
            .facts
            .canonical_mount_counts(package)
            .map_err(map_fact_error)?;
        let zygotes = self
            .facts
            .arm64_zygote_processes()
            .map_err(map_fact_error)?;
        let zygote = namespace_consensus(&mut self.executor, package, &zygotes)?;
        Ok(ViewProof::new(
            CanonicalView::new(canonical, counts),
            mirror,
            zygote,
        ))
    }

    fn slot_inodes(
        &mut self,
        package: &PackageName,
        slot_id: &SlotId,
    ) -> Result<Option<DataInodes>, ProbeError> {
        require_target(package, UserId::PRIMARY)?;
        self.facts
            .slot_inodes(package, slot_id)
            .map_err(map_fact_error)
    }

    fn user0_unlocked(&mut self) -> Result<bool, ProbeError> {
        self.bridge
            .query_device(ALLOWED_USER_ID)
            .map(|snapshot| snapshot.unlocked())
            .map_err(|error| map_bridge_error(&error))
    }
}

impl<R: BridgeCommandRunner, E: ProcessExecutor, F: SystemFacts> SystemPackageProbe<R, E, F> {
    fn refresh_package_snapshot(
        &mut self,
        package: &PackageName,
        user_id: UserId,
    ) -> Result<PackageSnapshot, ProbeError> {
        require_target(package, user_id)?;
        let snapshot = self
            .bridge
            .query_package(package.as_str(), ALLOWED_USER_ID)
            .map_err(|error| map_bridge_error(&error))?;
        let base = RuntimeLayout::slot_paths(package, &SlotId::base());
        if Path::new(snapshot.ce_data_path()) != base.ce()
            || Path::new(snapshot.de_data_path()) != base.de()
        {
            return Err(ProbeError::InvalidResponse);
        }
        self.cached_package = Some(snapshot.clone());
        Ok(snapshot)
    }
}

const fn require_target(_package: &PackageName, user_id: UserId) -> Result<(), ProbeError> {
    if user_id.get() == ALLOWED_USER_ID {
        Ok(())
    } else {
        Err(ProbeError::InvalidResponse)
    }
}
