#![allow(
    clippy::missing_docs_in_private_items,
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "deterministic fake platform for the host stress boundary"
)]

use std::os::unix::fs::PermissionsExt as _;
use tempfile::TempDir;
use uclone_slot_runtime::domain::{
    AppIdentity, BootId, CommitNonce, DataInodes, GateSnapshot, ManagedPackage,
    PackageEnabledState, PackageKey, PackageName, PackageObservation, SlotId, SlotView,
    TransactionId, UserId,
};
use uclone_slot_runtime::journal::JournalStore;
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::registry::RegistryStore;
use uclone_slot_runtime::runtime::{
    FaultInjector, PlatformError, RuntimeBackend, RuntimeStores, SwitchCoordinator, SwitchMetadata,
    SwitchRequest,
};

const PACKAGE: &str = "com.uclone.slotprobe";
const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn base_inodes() -> DataInodes {
    DataInodes::new(10_001, 20_001).unwrap()
}

#[derive(Debug)]
pub(super) struct StressBackend {
    pub(super) current: SlotView,
    pub(super) gate_held: bool,
    pub(super) lease_retired: usize,
    pub(super) active_mounts: usize,
    pub(super) peak_mounts: usize,
    pub(super) mount_apply_calls: usize,
}

impl StressBackend {
    pub(super) fn new() -> Self {
        Self {
            current: SlotView::new(SlotId::base(), base_inodes()),
            gate_held: false,
            lease_retired: 0,
            active_mounts: 0,
            peak_mounts: 0,
            mount_apply_calls: 0,
        }
    }

    fn identity() -> AppIdentity {
        AppIdentity::new(10_321, SIGNATURE, 1, "/data/app/slotprobe/base.apk").unwrap()
    }

    pub(super) fn managed(&self) -> ManagedPackage {
        ManagedPackage::new(
            PackageKey::new(PackageName::parse(PACKAGE).unwrap(), UserId::PRIMARY),
            Self::identity(),
            base_inodes(),
            self.current.clone(),
            LifecycleState::Normal,
        )
        .unwrap()
    }
}

impl RuntimeBackend for StressBackend {
    fn observe_package(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<PackageObservation, PlatformError> {
        Ok(PackageObservation::new(
            Self::identity(),
            base_inodes(),
            self.current.inodes(),
            self.current.inodes(),
            false,
        ))
    }

    fn capture_gate_snapshot(
        &mut self,
        _package: &ManagedPackage,
    ) -> Result<GateSnapshot, PlatformError> {
        Ok(GateSnapshot::new(PackageEnabledState::Default, false))
    }

    fn acquire_gate(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.gate_held = true;
        Ok(())
    }

    fn verify_gate_held(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.gate_held
            .then_some(())
            .ok_or_else(|| PlatformError::VerifyGateHeld {
                detail: "stress_gate_not_held".to_owned(),
            })
    }

    fn quiesce_processes(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        Ok(())
    }

    fn apply_slot_view(
        &mut self,
        _package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        if !self.gate_held {
            return Err(PlatformError::ApplySlotView {
                detail: "stress_mount_without_gate".to_owned(),
            });
        }
        self.current = view.clone();
        // One stable CE and one stable DE mount are replaced in-place each switch.
        self.active_mounts = 2;
        self.peak_mounts = self.peak_mounts.max(self.active_mounts);
        self.mount_apply_calls += 1;
        Ok(())
    }

    fn verify_slot_view(
        &mut self,
        _package: &ManagedPackage,
        view: &SlotView,
    ) -> Result<(), PlatformError> {
        self.current
            .eq(view)
            .then_some(())
            .ok_or_else(|| PlatformError::VerifySlotView {
                detail: "stress_view_mismatch".to_owned(),
            })
    }

    fn restore_gate(
        &mut self,
        _package: &ManagedPackage,
        _snapshot: GateSnapshot,
    ) -> Result<(), PlatformError> {
        self.gate_held = false;
        Ok(())
    }

    fn retire_gate_lease(&mut self, _package: &ManagedPackage) -> Result<(), PlatformError> {
        self.lease_retired += 1;
        Ok(())
    }
}

pub(super) fn new_coordinator() -> (TempDir, SwitchCoordinator<StressBackend>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let journal = JournalStore::new(root.path().join("journal")).unwrap();
    let registry = RegistryStore::new(root.path().join("registry")).unwrap();
    let stores = RuntimeStores::new(journal, registry);
    (
        root,
        SwitchCoordinator::with_fault_injector(
            StressBackend::new(),
            stores,
            FaultInjector::disabled(),
        ),
    )
}

pub(super) fn request(backend: &StressBackend, index: usize) -> (SwitchRequest, SlotId) {
    let managed = backend.managed();
    let target = if managed.active_slot().is_base() {
        SlotView::new(
            SlotId::parse("work").unwrap(),
            DataInodes::new(30_001, 40_001).unwrap(),
        )
    } else {
        SlotView::new(SlotId::base(), base_inodes())
    };
    let transaction_id = TransactionId::parse(&format!("tx-stress-{index:04}")).unwrap();
    let nonce = CommitNonce::parse(&format!("nonce-stress-{index:04}")).unwrap();
    let boot_id = BootId::parse("boot-stress-0001").unwrap();
    (
        SwitchRequest::new(
            managed,
            target.clone(),
            SwitchMetadata::new(transaction_id, nonce, boot_id),
        ),
        target.slot_id().clone(),
    )
}
