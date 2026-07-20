#![allow(
    clippy::redundant_pub_crate,
    reason = "integration-test roots need pub(super) access to this private fixture module"
)]

use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, SlotView, TransactionId, UserId,
};
use uclone_slot_runtime::journal::TransactionSpec;
use uclone_slot_runtime::lifecycle::LifecycleState;

#[allow(
    dead_code,
    reason = "shared support is compiled independently by integration test crates"
)]
pub(super) fn secure_temp_dir(root: &tempfile::TempDir) {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
}

const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub(super) struct PackageContract {
    identity: AppIdentity,
    lifecycle_state: LifecycleState,
}

impl PackageContract {
    pub(super) const fn new(identity: AppIdentity, lifecycle_state: LifecycleState) -> Self {
        Self {
            identity,
            lifecycle_state,
        }
    }
}

pub(super) struct ManagedFixture {
    base: DataInodes,
    active: SlotView,
    contract: PackageContract,
}

impl ManagedFixture {
    pub(super) const fn new(base: DataInodes, active: SlotView, contract: PackageContract) -> Self {
        Self {
            base,
            active,
            contract,
        }
    }

    fn build(self) -> ManagedPackage {
        ManagedPackage::new(
            PackageKey::new(
                PackageName::parse("com.uclone.slotprobe").unwrap(),
                UserId::PRIMARY,
            ),
            self.contract.identity,
            self.base,
            self.active,
            self.contract.lifecycle_state,
        )
        .unwrap()
    }
}

pub(super) fn managed_with_contract(fixture: ManagedFixture) -> ManagedPackage {
    fixture.build()
}

pub(super) struct TransactionViews {
    base: DataInodes,
    previous: SlotView,
    target: SlotView,
}

impl TransactionViews {
    pub(super) const fn new(base: DataInodes, previous: SlotView, target: SlotView) -> Self {
        Self {
            base,
            previous,
            target,
        }
    }
}

pub(super) struct TransactionFixture {
    transaction_id: String,
    views: TransactionViews,
    contract: PackageContract,
    boot_id: String,
}

impl TransactionFixture {
    pub(super) fn new(transaction_id: &str, views: TransactionViews, boot_id: &str) -> Self {
        Self {
            transaction_id: transaction_id.to_owned(),
            views,
            contract: PackageContract::new(
                AppIdentity::new(10_321, SIGNATURE, 1, "/data/app/slotprobe/base.apk").unwrap(),
                LifecycleState::Normal,
            ),
            boot_id: boot_id.to_owned(),
        }
    }

    pub(super) fn with_contract(self, contract: PackageContract) -> Self {
        Self { contract, ..self }
    }

    fn build(self) -> TransactionSpec {
        let managed = managed_with_contract(ManagedFixture::new(
            self.views.base,
            self.views.previous,
            self.contract,
        ));
        TransactionSpec::new(
            TransactionId::parse(&self.transaction_id).unwrap(),
            managed,
            self.views.target,
            GateSnapshot::new(PackageEnabledState::Default, false),
            &self.boot_id,
        )
        .unwrap()
    }
}

pub(super) fn transaction_spec(fixture: TransactionFixture) -> TransactionSpec {
    fixture.build()
}

pub(super) fn transaction_spec_with_contract(fixture: TransactionFixture) -> TransactionSpec {
    fixture.build()
}
