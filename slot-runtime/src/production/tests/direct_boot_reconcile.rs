use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::super::composition::ProductionPlatform;
use super::super::metadata::SystemMetadataSource;
use super::orphan_gate::{probe, runtime::OrphanGateRuntime, stores};
use crate::android::{AndroidMaterializer, SystemMaterializerExecutor};
use crate::domain::{
    CommitNonce, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey, PackageName,
    PackageSupportLevel, SlotId, SlotView, TransactionId, UserId,
};
use crate::enrollment_attempt::CommitProof;
use crate::journal::TransactionSpec;
use crate::lifecycle::LifecycleState;
use crate::protocol::ALLOWED_PACKAGE;
use crate::reconcile::{ReconcileOutcome, ReconcileReason};
use crate::registry::PackageRevision;

#[test]
fn conditional_direct_boot_active_slot_reboot_keeps_gate_held() {
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
            PackageSupportLevel::DirectBootConditional,
            true,
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
    let spec = TransactionSpec::new(
        TransactionId::parse("conditional-reboot").unwrap(),
        managed.clone(),
        SlotView::new(SlotId::parse("preview").unwrap(), preview),
        GateSnapshot::new(PackageEnabledState::Default, false),
        "boot-conditional",
    )
    .unwrap();
    stores
        .registry
        .append(
            &PackageRevision::committed(
                &spec,
                base,
                CommitNonce::parse("conditional-nonce").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let snapshot = GateSnapshot::new(PackageEnabledState::Default, false);
    let runtime = OrphanGateRuntime::enrolled(snapshot, managed);
    let materializer = AndroidMaterializer::new(
        SystemMaterializerExecutor,
        probe::NativeBaseProbe::direct_boot(),
    );
    let mut platform = ProductionPlatform {
        runtime,
        materializer,
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::direct_boot()),
        metadata: SystemMetadataSource::new(),
        stores,
    };

    let outcome = platform.do_reconcile(&key).unwrap();

    assert_eq!(
        outcome,
        ReconcileOutcome::RecoveryRequired(ReconcileReason::ConditionalDirectBoot),
    );
    assert!(platform.runtime().held);
    assert_eq!(platform.runtime().restored, None);
}

#[test]
fn unlock_transition_requires_a_fresh_policy_checked_reconcile() {
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
        .compatibility_policy
        .create(
            key.package_name(),
            managed.identity(),
            PackageSupportLevel::DirectBootConditional,
            true,
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
    let snapshot = GateSnapshot::new(PackageEnabledState::Default, false);
    stores.attempts.create_pending(&key, snapshot).unwrap();
    let state = stores.package_state.initialize(&key).unwrap();
    let digests = stores.published_digests(&key, &state).unwrap();
    let proof = CommitProof::new(
        managed.clone(),
        &digests.enrollment,
        &digests.compatibility_policy,
        &digests.base_catalog,
        &digests.package_state,
    )
    .unwrap();
    stores.attempts.commit(&key, proof).unwrap();
    let runtime = OrphanGateRuntime::enrolled(snapshot, managed);
    let materializer = AndroidMaterializer::new(
        SystemMaterializerExecutor,
        probe::NativeBaseProbe::locked_direct_boot(),
    );
    let mut platform = ProductionPlatform {
        runtime,
        materializer,
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::locked_direct_boot()),
        metadata: SystemMetadataSource::new(),
        stores,
    };

    let outcome = platform.do_reconcile(&key).unwrap();

    assert_eq!(outcome, ReconcileOutcome::Locked);
    assert!(platform.runtime().held);
    assert_eq!(platform.runtime().restored, None);

    platform.probe.borrow_mut().set_user_unlocked(true);
    let unlocked = platform.do_reconcile(&key).unwrap();

    assert_eq!(unlocked, ReconcileOutcome::RestoredBase);
    assert!(!platform.runtime().held);
    assert_eq!(platform.runtime().restored, Some(snapshot));
}
