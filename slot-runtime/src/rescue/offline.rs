use std::path::Path;

use crate::domain::{GateSnapshot, PackageKey};
use crate::reconcile::{NativeBaseRecoveryBackend, RecoveryBackend};

use super::anchors::{LiveDecision, RescueAnchors};
use super::coordinator::RescueCoordinator;
use super::offline_roots::{RescueRoots, management_artifacts_present, supported};
use super::{
    NoRescueFault, RescueError, RescueExecution, RescueFaultInjector, RescueJournalStore,
    RescueMetadataSource, RescuePhase, RescueSpec, RescueStartup,
};

#[doc = "Independent emergency platform that never opens ordinary mutable stores."]
#[derive(Debug)]
pub struct OfflineRescuePlatform<B, T, F = NoRescueFault> {
    backend: B,
    metadata: T,
    faults: F,
    roots: RescueRoots,
}

impl<B, T> OfflineRescuePlatform<B, T, NoRescueFault> {
    #[doc = "Creates the production platform using only compiled fixed roots."]
    pub fn open_fixed(backend: B, metadata: T) -> Self {
        Self {
            backend,
            metadata,
            faults: NoRescueFault,
            roots: RescueRoots::fixed(),
        }
    }
}

impl<B, T, F> OfflineRescuePlatform<B, T, F> {
    #[doc(hidden)]
    pub fn with_dependencies(
        backend: B,
        metadata: T,
        faults: F,
        enrollment_root: impl AsRef<Path>,
        catalog_root: impl AsRef<Path>,
        journal_root: impl AsRef<Path>,
    ) -> Self {
        Self {
            backend,
            metadata,
            faults,
            roots: RescueRoots::with_roots(enrollment_root, catalog_root, journal_root),
        }
    }

    #[doc = "Returns the injected backend for deterministic evidence inspection."]
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    #[doc = "Consumes the platform and returns the injected dependencies."]
    pub fn into_dependencies(self) -> (B, T, F) {
        (self.backend, self.metadata, self.faults)
    }
}

impl<B, T, F> OfflineRescuePlatform<B, T, F>
where
    B: RecoveryBackend + NativeBaseRecoveryBackend,
    T: RescueMetadataSource,
    F: RescueFaultInjector,
{
    #[doc = "Executes or resumes an independent key-only native-base rescue."]
    pub fn rescue_to_base(&mut self, key: &PackageKey) -> RescueExecution {
        if !supported(key) {
            return RescueExecution::RecoveryRequired;
        }
        let Ok(journal) = RescueJournalStore::new(&self.roots.journal) else {
            return self.contain(key, RescueExecution::RecoveryRequired);
        };
        let Ok(anchors) = RescueAnchors::load(&self.roots.enrollment, &self.roots.catalog, key)
        else {
            return self.contain(key, RescueExecution::RecoveryRequired);
        };
        match anchors.validate_live(&mut self.backend) {
            LiveDecision::Valid => {}
            LiveDecision::RecoveryRequired => {
                return self.contain(key, RescueExecution::RecoveryRequired);
            }
            LiveDecision::Quarantined => {
                return self.contain(key, RescueExecution::Quarantined);
            }
        }
        let Ok(transaction) = journal.load() else {
            return self.contain(key, RescueExecution::RecoveryRequired);
        };
        let spec = match transaction {
            Some(transaction) => {
                if !anchors.matches_spec(transaction.spec()) {
                    return self.contain(key, RescueExecution::RecoveryRequired);
                }
                transaction.spec().clone()
            }
            None => match self.prepare(&journal, &anchors, key) {
                Ok(value) => value,
                Err(RescueError::InjectedCrash(_)) => {
                    return RescueExecution::RecoveryRequired;
                }
                Err(_) => return self.contain(key, RescueExecution::RecoveryRequired),
            },
        };
        RescueCoordinator::new(&mut self.backend, &journal, &mut self.faults)
            .resume(anchors.base(), &spec)
    }

    #[doc = "Holds the package gate before ordinary stores open when management artifacts exist."]
    pub fn hold_startup_gate(&mut self, key: &PackageKey) -> Result<bool, RescueError> {
        if !supported(key) {
            return Err(RescueError::UnsupportedPackage);
        }
        if !management_artifacts_present(&self.roots, key)? {
            return Ok(false);
        }
        self.backend
            .emergency_gate(key.package_name())
            .map_err(platform_error)?;
        Ok(true)
    }

    #[doc = "Reconciles rescue state before any ordinary store may be opened."]
    pub fn reconcile_startup(&mut self, key: &PackageKey) -> RescueStartup {
        if !supported(key) {
            return RescueStartup::RecoveryRequired;
        }
        let Ok(journal) = RescueJournalStore::new(&self.roots.journal) else {
            return self.startup_contain(key, RescueStartup::RecoveryRequired);
        };
        match journal.load() {
            Ok(None) => RescueStartup::OpenOrdinary,
            Ok(Some(_)) => match self.rescue_to_base(key) {
                RescueExecution::CompletedBase => RescueStartup::BaseRetired,
                RescueExecution::RecoveryRequired => RescueStartup::RecoveryRequired,
                RescueExecution::Quarantined => RescueStartup::Quarantined,
                RescueExecution::ContainmentFailed => RescueStartup::ContainmentFailed,
            },
            Err(_) => self.startup_contain(key, RescueStartup::RecoveryRequired),
        }
    }

    fn prepare(
        &mut self,
        journal: &RescueJournalStore,
        anchors: &RescueAnchors,
        key: &PackageKey,
    ) -> Result<RescueSpec, RescueError> {
        let gate = self.capture_gate(anchors)?;
        let metadata = self.metadata.next_rescue()?;
        let (enrollment, base_manifest) = anchors.spec_digests();
        let spec = RescueSpec::new(
            metadata.rescue_id().clone(),
            key.clone(),
            anchors.base().identity().clone(),
            anchors.base().base_inodes(),
            gate,
            enrollment,
            base_manifest,
            metadata.boot_id().as_str(),
            metadata.commit_nonce().clone(),
        )?;
        journal.begin(&spec)?;
        self.faults.after_phase(RescuePhase::Prepared)?;
        Ok(spec)
    }

    fn capture_gate(&mut self, anchors: &RescueAnchors) -> Result<GateSnapshot, RescueError> {
        if let Ok(snapshot) = self.backend.capture_gate_snapshot(anchors.base()) {
            return Ok(snapshot);
        }
        let snapshot = self
            .backend
            .emergency_gate_if_leased(anchors.base().package_name())
            .map_err(platform_error)?
            .ok_or_else(|| RescueError::Corrupt("rescue gate capture failed".to_owned()))?;
        self.backend
            .confirm_emergency_gate(anchors.base())
            .map_err(platform_error)?;
        Ok(snapshot)
    }

    fn contain(&mut self, key: &PackageKey, outcome: RescueExecution) -> RescueExecution {
        if self.backend.emergency_gate(key.package_name()).is_ok() {
            outcome
        } else {
            RescueExecution::ContainmentFailed
        }
    }

    fn startup_contain(&mut self, key: &PackageKey, outcome: RescueStartup) -> RescueStartup {
        if self.backend.emergency_gate(key.package_name()).is_ok() {
            outcome
        } else {
            RescueStartup::ContainmentFailed
        }
    }
}

fn platform_error(error: impl Into<crate::runtime::PlatformError>) -> RescueError {
    let error = error.into();
    RescueError::Corrupt(error.to_string())
}
