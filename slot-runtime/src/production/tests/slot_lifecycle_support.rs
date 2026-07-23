use crate::domain::{
    BootId, CommitNonce, ManagedPackage, PackageKey, PackageName, SlotId, SlotView, TransactionId,
    UserId,
};
use crate::lifecycle::LifecycleState;
use crate::production::metadata::MetadataSource;
use crate::runtime::SwitchMetadata;
use crate::service::ServiceError;

use super::orphan_gate::{probe, stores};

mod materializer;

pub(super) use materializer::TrackingMaterializer;

pub(super) fn ready_package(
    root: &std::path::Path,
) -> (
    super::super::stores::ProductionStores,
    PackageKey,
    ManagedPackage,
) {
    let stores = stores(root);
    let key = PackageKey::new(
        PackageName::parse(crate::protocol::ALLOWED_PACKAGE).unwrap(),
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
            crate::domain::PackageSupportLevel::Supported,
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
    stores.package_state.initialize(&key).unwrap();
    (stores, key, managed)
}

#[derive(Debug)]
pub(super) struct FixedMetadata {
    slot: SlotId,
}

impl FixedMetadata {
    pub(super) fn new(slot: &str) -> Self {
        Self {
            slot: SlotId::parse(slot).unwrap(),
        }
    }
}

impl MetadataSource for FixedMetadata {
    fn next(&mut self) -> Result<SwitchMetadata, ServiceError> {
        Ok(SwitchMetadata::new(
            TransactionId::parse("tx-slot-lifecycle-test").unwrap(),
            CommitNonce::parse("nonce-slot-lifecycle-test").unwrap(),
            BootId::parse("boot-slot-lifecycle-test").unwrap(),
        ))
    }

    fn next_slot_id(&mut self) -> Result<SlotId, ServiceError> {
        Ok(self.slot.clone())
    }
}
