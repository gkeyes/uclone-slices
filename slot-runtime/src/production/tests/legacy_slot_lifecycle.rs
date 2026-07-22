use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::super::composition::ProductionPlatform;
use super::super::metadata::SystemMetadataSource;
use super::orphan_gate::{probe, runtime::OrphanGateRuntime, stores};
use crate::domain::{
    GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey, PackageName,
    PackageSupportLevel, SlotId, SlotView, UserId,
};
use crate::lifecycle::LifecycleState;
use crate::materializer::{
    ArtifactState, BackendFailure, BaseAnchor, DataBytes, DataDomain, DirectoryAnchor,
    DomainCopyProof, MaterializationBackend, MaterializationPaths, SlotMaterializationProof,
};
use crate::protocol::ALLOWED_PACKAGE;
use crate::slot_metadata::SlotRecordState;

const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[test]
fn catalog_only_legacy_slot_can_be_deleted_after_returning_to_base() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let stores = stores(root.path());
    let key = PackageKey::new(
        PackageName::parse(ALLOWED_PACKAGE).unwrap(),
        UserId::PRIMARY,
    );
    let base = probe::base_inodes();
    let preview = crate::domain::DataInodes::new(977_507, 977_517).unwrap();
    let managed = ManagedPackage::new(
        key.clone(),
        probe::identity(),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap();
    stores.enrollment.create(&managed).unwrap();
    stores
        .compatibility_policy
        .create(
            key.package_name(),
            managed.identity(),
            PackageSupportLevel::Supported,
            false,
        )
        .unwrap();
    stores
        .catalog
        .create_base(
            key.clone(),
            base,
            managed.identity().clone(),
            probe::security_profile(),
        )
        .unwrap();
    stores
        .catalog
        .append_slot(
            &key,
            SlotId::parse("preview").unwrap(),
            preview,
            managed.identity(),
            managed.identity().version_code(),
            probe::security_profile(),
        )
        .unwrap();
    stores.package_state.initialize(&key).unwrap();
    let gate = GateSnapshot::new(PackageEnabledState::Default, false);
    let runtime = OrphanGateRuntime::enrolled(gate, managed);
    let mut platform = ProductionPlatform {
        runtime,
        materializer: DeletionMaterializer { ready: true },
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: SystemMetadataSource::new(),
        stores,
    };
    let preview = SlotId::parse("preview").unwrap();

    let result = platform.do_delete_slot(&key, &preview);

    assert!(result.is_ok(), "legacy preview deletion failed: {result:?}");
    assert!(!platform.materializer().ready);
    assert_eq!(
        platform
            .stores
            .slot_metadata
            .latest(key.package_name(), &preview)
            .unwrap()
            .unwrap()
            .state(),
        SlotRecordState::Deleted,
    );
}

#[derive(Debug)]
struct DeletionMaterializer {
    ready: bool,
}

impl MaterializationBackend for DeletionMaterializer {
    fn verify_gate_held(&mut self, _: &ManagedPackage) -> Result<(), BackendFailure> {
        Ok(())
    }

    fn verify_processes_quiesced(&mut self, _: &ManagedPackage) -> Result<(), BackendFailure> {
        Ok(())
    }

    fn capture_base_anchor(&mut self, _: &ManagedPackage) -> Result<BaseAnchor, BackendFailure> {
        BaseAnchor::new(
            probe::base_inodes(),
            DirectoryAnchor::new(11, DIGEST).unwrap(),
            DirectoryAnchor::new(12, DIGEST).unwrap(),
            probe::security_profile(),
            DataBytes::new(0, 0),
        )
        .map_err(|_| unused())
    }

    fn verify_capacity(
        &mut self,
        _: &ManagedPackage,
        _: &BaseAnchor,
    ) -> Result<(), BackendFailure> {
        Err(unused())
    }

    fn artifact_state(
        &mut self,
        _: &MaterializationPaths,
    ) -> Result<ArtifactState, BackendFailure> {
        Ok(if self.ready {
            ArtifactState::ReadyOnly
        } else {
            ArtifactState::Absent
        })
    }

    fn cleanup_artifacts(&mut self, _: &MaterializationPaths) -> Result<(), BackendFailure> {
        self.ready = false;
        Ok(())
    }

    fn create_staging(&mut self, _: &MaterializationPaths) -> Result<(), BackendFailure> {
        Err(unused())
    }

    fn copy_domain(
        &mut self,
        _: &ManagedPackage,
        _: &MaterializationPaths,
        _: DataDomain,
    ) -> Result<DomainCopyProof, BackendFailure> {
        Err(unused())
    }

    fn apply_security(
        &mut self,
        _: &ManagedPackage,
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
