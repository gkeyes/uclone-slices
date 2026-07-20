mod probe;
mod runtime;

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::super::composition::ProductionPlatform;
use super::super::metadata::SystemMetadataSource;
use super::super::stores::ProductionStores;
use crate::android::{AndroidMaterializer, SystemMaterializerExecutor};
use crate::catalog::CatalogStore;
use crate::domain::{
    GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey, PackageName, SlotId, SlotView,
    UserId,
};
use crate::enrollment::EnrollmentStore;
use crate::enrollment_attempt::EnrollmentAttemptStore;
use crate::journal::JournalStore;
use crate::lifecycle::LifecycleState;
use crate::package_state::{PackageStateReason, PackageStateStore};
use crate::protocol::ALLOWED_PACKAGE;
use crate::reconcile::ReconcileOutcome;
use crate::registry::RegistryStore;

use self::probe::NativeBaseProbe;
use self::runtime::OrphanGateRuntime;

#[test]
fn pristine_orphan_gate_restores_exact_state_and_retires_lease() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let snapshot = GateSnapshot::new(PackageEnabledState::DisabledUntilUsed, true);
    let runtime = OrphanGateRuntime::new(snapshot);
    let materializer = AndroidMaterializer::new(SystemMaterializerExecutor, NativeBaseProbe::new());
    let mut platform = ProductionPlatform {
        runtime,
        materializer,
        probe: std::cell::RefCell::new(NativeBaseProbe::new()),
        metadata: SystemMetadataSource::new(),
        stores: stores(root.path()),
    };
    let key = PackageKey::new(
        PackageName::parse(ALLOWED_PACKAGE).unwrap(),
        UserId::PRIMARY,
    );

    let outcome = platform.do_reconcile(&key).unwrap();

    assert_eq!(outcome, ReconcileOutcome::RestoredBase);
    assert_eq!(platform.runtime().restored, Some(snapshot));
    assert!(platform.runtime().retired);
    assert!(!platform.runtime().held);
    assert_eq!(platform.runtime().confirmations, 1);
    assert_eq!(platform.runtime().base_proofs, 1);
}

#[test]
fn proved_base_reconciliation_recovers_a_transient_boot_failure_state() {
    let root = tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let stores = stores(root.path());
    let key = PackageKey::new(
        PackageName::parse(ALLOWED_PACKAGE).unwrap(),
        UserId::PRIMARY,
    );
    let base = probe::base_inodes();
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
            crate::domain::DataInodes::new(977_507, 977_517).unwrap(),
            managed.identity(),
            managed.identity().version_code(),
            probe::security_profile(),
        )
        .unwrap();
    stores.package_state.initialize(&key).unwrap();
    stores
        .package_state
        .transition(
            &key,
            LifecycleState::Normal,
            LifecycleState::RecoveryRequired,
            PackageStateReason::ViewUncertain,
        )
        .unwrap();
    let state = stores.package_state.clone();
    let snapshot = GateSnapshot::new(PackageEnabledState::Default, false);
    let runtime = OrphanGateRuntime::enrolled(snapshot, managed);
    let materializer = AndroidMaterializer::new(SystemMaterializerExecutor, NativeBaseProbe::new());
    let mut platform = ProductionPlatform {
        runtime,
        materializer,
        probe: std::cell::RefCell::new(NativeBaseProbe::new()),
        metadata: SystemMetadataSource::new(),
        stores,
    };

    let outcome = platform.do_reconcile(&key).unwrap();

    assert_eq!(outcome, ReconcileOutcome::RestoredBase);
    assert_eq!(
        state.latest(&key).unwrap().unwrap().lifecycle_state(),
        LifecycleState::Normal,
    );
}

fn stores(root: &std::path::Path) -> ProductionStores {
    ProductionStores {
        enrollment: EnrollmentStore::new(root.join("enrollment")).unwrap(),
        attempts: EnrollmentAttemptStore::new(root.join("enrollment-attempts")).unwrap(),
        catalog: CatalogStore::new(root.join("catalog")).unwrap(),
        package_state: PackageStateStore::new(root.join("package-state")).unwrap(),
        journal: JournalStore::new(root.join("journal")).unwrap(),
        registry: RegistryStore::new(root.join("registry")).unwrap(),
        slot_metadata: crate::slot_metadata::SlotMetadataStore::new(root.join("slot-metadata"))
            .unwrap(),
    }
}
