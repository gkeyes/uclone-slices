use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::tempdir;

use super::super::composition::ProductionPlatform;
use super::super::metadata::SystemMetadataSource;
use super::super::reconciliation_validation::publications_match;
use super::orphan_gate::{probe, runtime::OrphanGateRuntime, stores};
use crate::android::{AndroidMaterializer, SystemMaterializerExecutor};
use crate::domain::{
    GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey, PackageName,
    PackageSupportLevel, SlotId, SlotView, UserId,
};
use crate::enrollment_attempt::CommitProof;
use crate::lifecycle::LifecycleState;
use crate::protocol::ALLOWED_PACKAGE;
use crate::reconcile::{ReconcileOutcome, ReconcileReason};

#[test]
fn policy_bytes_are_covered_by_the_authoritative_enrollment_anchor() {
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
    let gate = GateSnapshot::new(PackageEnabledState::Default, false);
    stores.attempts.create_pending(&key, gate).unwrap();
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
    let attempt = stores.attempts.load(&key).unwrap().unwrap();
    assert!(publications_match(&stores, &key, &attempt).unwrap());

    let policy = root
        .path()
        .join("compatibility-policy/packages")
        .join(ALLOWED_PACKAGE)
        .join("policy.json");
    let mut bytes = fs::read(&policy).unwrap();
    let last = bytes.last_mut().unwrap();
    *last ^= 1;
    fs::write(&policy, bytes).unwrap();
    fs::set_permissions(&policy, fs::Permissions::from_mode(0o600)).unwrap();

    assert!(!publications_match(&stores, &key, &attempt).unwrap());

    let runtime = OrphanGateRuntime::enrolled(gate, managed);
    let materializer =
        AndroidMaterializer::new(SystemMaterializerExecutor, probe::NativeBaseProbe::new());
    let mut platform = ProductionPlatform {
        runtime,
        materializer,
        probe: std::cell::RefCell::new(probe::NativeBaseProbe::new()),
        metadata: SystemMetadataSource::new(),
        stores,
        recovery_overrides: BTreeSet::default(),
    };
    let outcome = platform.do_reconcile(&key).unwrap();

    assert_eq!(
        outcome,
        ReconcileOutcome::RecoveryRequired(ReconcileReason::EnrollmentMetadata),
    );
    assert!(platform.runtime().held);
    assert_eq!(platform.runtime().restored, None);
    assert!(platform.stores.attempts.load(&key).unwrap().is_some());
}
