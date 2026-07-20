#![doc = "Shared fake platform and durable fixtures for reconciliation tests."]
#![allow(
    clippy::missing_panics_doc,
    clippy::unwrap_used,
    reason = "validated reconciliation test fixtures fail the invoking test immediately"
)]

use tempfile::TempDir;
use uclone_slot_runtime::domain::{
    AppIdentity, CommitNonce, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState,
    PackageKey, PackageName, SlotId, SlotView, TransactionId, UserId,
};
use uclone_slot_runtime::enrollment::EnrollmentStore;
use uclone_slot_runtime::journal::{JournalEvent, JournalStore, TransactionSpec};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::reconcile::Reconciler;
use uclone_slot_runtime::registry::{PackageRevision, RegistryStore};

#[doc = "Makes a temporary parent satisfy the production 0700 store boundary."]
pub fn secure_temp_dir(root: &TempDir) {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[path = "support/fake.rs"]
#[doc = "Deterministic platform backend and observable call counters."]
pub mod fake;

pub use fake::FakeBackend;

const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

#[derive(Debug)]
#[doc = "Isolated durable stores shared by one reconciliation scenario."]
pub struct TestStores {
    _root: TempDir,
    #[doc = "Enrollment fixture store."]
    pub enrollment: EnrollmentStore,
    #[doc = "Journal fixture store."]
    pub journal: JournalStore,
    #[doc = "Registry fixture store."]
    pub registry: RegistryStore,
}

impl TestStores {
    #[doc = "Creates isolated stores containing one valid base enrollment."]
    pub fn new(managed: &ManagedPackage) -> Self {
        let root = TempDir::new().unwrap();
        secure_temp_dir(&root);
        let enrollment = EnrollmentStore::new(root.path().join("enrollment")).unwrap();
        enrollment.create(managed).unwrap();
        let journal = JournalStore::new(root.path().join("journal")).unwrap();
        let registry = RegistryStore::new(root.path().join("registry")).unwrap();
        Self {
            _root: root,
            enrollment,
            journal,
            registry,
        }
    }

    #[doc = "Builds a reconciler over clones of the isolated stores."]
    pub fn reconciler(&self, backend: FakeBackend) -> Reconciler<FakeBackend> {
        Reconciler::new(
            backend,
            self.enrollment.clone(),
            self.journal.clone(),
            self.registry.clone(),
        )
    }
}

#[doc = "Returns the allowlisted base enrollment fixture."]
pub fn base_package() -> ManagedPackage {
    let base = base_inodes();
    ManagedPackage::new(
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
        identity(SIGNATURE),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap()
}

#[doc = "Returns a valid application identity with the supplied signing digest."]
pub fn identity(signature: &str) -> AppIdentity {
    AppIdentity::new(10_321, signature, 1, "/data/app/slotprobe/base.apk").unwrap()
}

#[doc = "Returns stable native base inode anchors."]
pub fn base_inodes() -> DataInodes {
    DataInodes::new(101, 201).unwrap()
}

#[doc = "Returns the paired non-base work view."]
pub fn work_view() -> SlotView {
    SlotView::new(
        SlotId::parse("work").unwrap(),
        DataInodes::new(301, 401).unwrap(),
    )
}

#[doc = "Returns a valid slot transaction fixture."]
pub fn transaction(id: &str) -> TransactionSpec {
    TransactionSpec::new(
        TransactionId::parse(id).unwrap(),
        base_package(),
        work_view(),
        GateSnapshot::new(PackageEnabledState::Enabled, true),
        "boot-reconcile-0001",
    )
    .unwrap()
}

#[doc = "Persists a transaction stopped before its commit point."]
pub fn append_precommit(journal: &JournalStore, spec: &TransactionSpec) {
    journal.create(spec).unwrap();
    for event in [
        JournalEvent::GateHeld,
        JournalEvent::ProcessesQuiesced,
        JournalEvent::Applying,
    ] {
        journal.append(spec.transaction_id(), event).unwrap();
    }
}

#[doc = "Persists a transaction stopped at its commit-pending point."]
pub fn append_commit_pending(journal: &JournalStore, spec: &TransactionSpec) {
    append_precommit(journal, spec);
    journal
        .append(spec.transaction_id(), JournalEvent::ViewVerified)
        .unwrap();
    journal
        .append(
            spec.transaction_id(),
            JournalEvent::Committing {
                nonce: commit_nonce(),
            },
        )
        .unwrap();
}

#[doc = "Publishes the transaction target as the current Registry revision."]
pub fn publish_target(registry: &RegistryStore, spec: &TransactionSpec) {
    let draft = PackageRevision::committed(spec, spec.base_inodes(), commit_nonce()).unwrap();
    registry.append(&draft).unwrap();
}

fn commit_nonce() -> CommitNonce {
    CommitNonce::parse("nonce-reconcile-0001").unwrap()
}
