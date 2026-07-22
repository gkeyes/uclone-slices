use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::{cell::Cell, rc::Rc};

use tempfile::tempdir;

use super::super::composition::ProductionPlatform;
use super::super::metadata::SystemMetadataSource;
use super::orphan_gate::{probe, runtime::OrphanGateRuntime, stores};
use crate::domain::{GateSnapshot, PackageEnabledState, PackageKey, PackageName, UserId};
use crate::enrollment_attempt::EnrollmentAttemptPhase;
use crate::materializer::{
    ArtifactState, BackendFailure, BaseAnchor, DataBytes, DataDomain, DirectoryAnchor,
    DomainCopyProof, MaterializationBackend, MaterializationPaths, SlotMaterializationProof,
};
use crate::protocol::ALLOWED_PACKAGE;
use crate::service::{EnrollmentPublicationError, ServiceError};

const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn final_candidate_recheck_rejects_install_that_starts_after_base_capture() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let key = PackageKey::new(
        PackageName::parse(ALLOWED_PACKAGE).unwrap(),
        UserId::PRIMARY,
    );
    let snapshot = GateSnapshot::new(PackageEnabledState::Default, false);
    let captured = Rc::new(Cell::new(false));
    let stores = stores(root.path());
    stores.attempts.create_pending(&key, snapshot).unwrap();
    let mut runtime = OrphanGateRuntime::new(snapshot);
    runtime.held = true;
    let mut platform = ProductionPlatform {
        runtime,
        materializer: EnrollmentMaterializer {
            captured: Rc::clone(&captured),
        },
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::pending_after_capture(captured)),
        metadata: SystemMetadataSource::new(),
        stores,
    };

    let result = platform.do_enroll(&key, false);

    assert!(matches!(
        result,
        Err(EnrollmentPublicationError::Unpublished(
            ServiceError::RecoveryRequired
        ))
    ));
    assert!(
        platform
            .stores
            .enrollment
            .load(key.package_name())
            .unwrap()
            .is_none()
    );
    assert_eq!(
        platform
            .stores
            .attempts
            .load(&key)
            .unwrap()
            .unwrap()
            .phase(),
        EnrollmentAttemptPhase::Pending,
    );
    assert!(platform.runtime().held);
    assert_eq!(platform.runtime().restored, None);
    assert!(!platform.runtime().retired);
}

#[derive(Debug)]
struct EnrollmentMaterializer {
    captured: Rc<Cell<bool>>,
}

impl MaterializationBackend for EnrollmentMaterializer {
    fn verify_gate_held(
        &mut self,
        _: &crate::domain::ManagedPackage,
    ) -> Result<(), BackendFailure> {
        Ok(())
    }

    fn verify_processes_quiesced(
        &mut self,
        _: &crate::domain::ManagedPackage,
    ) -> Result<(), BackendFailure> {
        Ok(())
    }

    fn capture_base_anchor(
        &mut self,
        _: &crate::domain::ManagedPackage,
    ) -> Result<BaseAnchor, BackendFailure> {
        self.captured.set(true);
        BaseAnchor::new(
            probe::base_inodes(),
            DirectoryAnchor::new(11, DIGEST).unwrap(),
            DirectoryAnchor::new(12, DIGEST).unwrap(),
            probe::security_profile(),
            DataBytes::new(0, 0),
        )
        .map_err(|_| BackendFailure::new("invalid test base anchor"))
    }

    fn verify_capacity(
        &mut self,
        _: &crate::domain::ManagedPackage,
        _: &BaseAnchor,
    ) -> Result<(), BackendFailure> {
        Err(unused())
    }

    fn artifact_state(
        &mut self,
        _: &MaterializationPaths,
    ) -> Result<ArtifactState, BackendFailure> {
        Err(unused())
    }

    fn cleanup_artifacts(&mut self, _: &MaterializationPaths) -> Result<(), BackendFailure> {
        Err(unused())
    }

    fn create_staging(&mut self, _: &MaterializationPaths) -> Result<(), BackendFailure> {
        Err(unused())
    }

    fn copy_domain(
        &mut self,
        _: &crate::domain::ManagedPackage,
        _: &MaterializationPaths,
        _: DataDomain,
    ) -> Result<DomainCopyProof, BackendFailure> {
        Err(unused())
    }

    fn apply_security(
        &mut self,
        _: &crate::domain::ManagedPackage,
        _: &MaterializationPaths,
        _: &crate::catalog::SecurityProfileProof,
    ) -> Result<(), BackendFailure> {
        Err(unused())
    }

    fn inspect_staging(
        &mut self,
        _: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure> {
        Err(unused())
    }

    fn sync_domain(
        &mut self,
        _: &MaterializationPaths,
        _: DataDomain,
    ) -> Result<(), BackendFailure> {
        Err(unused())
    }

    fn publish_ready(
        &mut self,
        _: &MaterializationPaths,
    ) -> Result<SlotMaterializationProof, BackendFailure> {
        Err(unused())
    }
}

fn unused() -> BackendFailure {
    BackendFailure::new("unexpected materialization operation")
}
