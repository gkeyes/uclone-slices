use super::super::orphan_gate::{probe, stores as fixture_stores};
use super::fixture::{Expected, Fixture};
use super::fixture_data::{base_parts, default_gate, finish, key, root};
use super::matrix_probe::MatrixProbe;
use crate::domain::{
    AppIdentity, DataInodes, PackageCompatibility, PackageObservation, SlotId, SlotView,
    TransactionId,
};
use crate::journal::TransactionSpec;
use crate::lifecycle::LifecycleState;
use crate::package_state::PackageStateReason;

pub(super) fn orphan() -> Fixture {
    let root = root();
    let stores = fixture_stores(root.path());
    let key = key();
    stores
        .compatibility_policy
        .create(
            key.package_name(),
            &probe::identity(),
            crate::domain::PackageSupportLevel::Supported,
            false,
        )
        .unwrap();
    finish(
        root,
        stores,
        key,
        MatrixProbe::healthy(default_gate()),
        Expected::Recovery,
    )
}

pub(super) fn unfinished() -> Fixture {
    let (root, stores, key, managed) = base_parts();
    let target = SlotView::new(
        SlotId::parse("slot-unfinished-state").unwrap(),
        DataInodes::new(1_420_001, 1_420_002).unwrap(),
    );
    let spec = TransactionSpec::new(
        TransactionId::parse("tx-unfinished-state").unwrap(),
        managed,
        target,
        default_gate(),
        "boot-unfinished-state",
    )
    .unwrap();
    stores.journal.create(&spec).unwrap();
    finish(
        root,
        stores,
        key,
        MatrixProbe::healthy(default_gate()),
        Expected::Recovery,
    )
}

pub(super) fn identity_mismatch() -> Fixture {
    let (root, stores, key, _) = base_parts();
    let old = probe::identity();
    let changed = AppIdentity::new(
        old.uid() + 1,
        old.signature_sha256(),
        old.version_code(),
        old.code_path(),
    )
    .unwrap();
    let base = probe::base_inodes();
    let observation = PackageObservation::new(changed, base, base, base, false);
    finish(
        root,
        stores,
        key,
        MatrixProbe::with_observation(observation),
        Expected::Quarantined,
    )
}

pub(super) fn blocked() -> Fixture {
    let (root, stores, key, _) = base_parts();
    let base = probe::base_inodes();
    let observation = PackageObservation::with_compatibility(
        probe::identity(),
        base,
        base,
        base,
        false,
        PackageCompatibility::new(true, false, false),
    );
    finish(
        root,
        stores,
        key,
        MatrixProbe::with_observation(observation),
        Expected::Quarantined,
    )
}

pub(super) fn transition_recovery(
    stores: &super::super::super::stores::ProductionStores,
    key: &crate::domain::PackageKey,
) {
    stores
        .package_state
        .transition(
            key,
            LifecycleState::Normal,
            LifecycleState::RecoveryRequired,
            PackageStateReason::ViewUncertain,
        )
        .unwrap();
}

pub(super) fn transition_quarantine(
    stores: &super::super::super::stores::ProductionStores,
    key: &crate::domain::PackageKey,
) {
    stores
        .package_state
        .transition(
            key,
            LifecycleState::Normal,
            LifecycleState::Quarantined,
            PackageStateReason::IdentityChanged,
        )
        .unwrap();
}

pub(super) fn transition_drift(
    stores: &super::super::super::stores::ProductionStores,
    key: &crate::domain::PackageKey,
) {
    stores
        .package_state
        .transition(
            key,
            LifecycleState::Normal,
            LifecycleState::LifecycleDrifted,
            PackageStateReason::LifecycleDrift,
        )
        .unwrap();
}
