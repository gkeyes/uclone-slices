use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use uclone_slot_runtime::catalog::{CatalogStore, PathSecurityProof, SecurityProfileProof};
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, ManagedPackage, PackageKey, PackageName, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::enrollment::EnrollmentStore;
use uclone_slot_runtime::journal::JournalStore;
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::package_state::PackageStateStore;
use uclone_slot_runtime::registry::RegistryStore;
use uclone_slot_runtime::rescue::{OfflineRescuePlatform, RescueFaultInjector, RescueJournalStore};

use super::{FakeMetadata, FakeRescueBackend};

pub(crate) const SIGNATURE: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const POLICY: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[derive(Debug)]
pub(crate) struct Fixture {
    _root: TempDir,
    enrollment: PathBuf,
    catalog: PathBuf,
    rescue: PathBuf,
    ordinary: PathBuf,
    managed: ManagedPackage,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        let root = TempDir::new().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let enrollment = root.path().join("enrollment");
        let catalog = root.path().join("catalog");
        let rescue = root.path().join("rescue-journal");
        let ordinary = root.path().join("ordinary");
        fs::create_dir(&ordinary).unwrap();
        fs::set_permissions(&ordinary, fs::Permissions::from_mode(0o700)).unwrap();
        let managed = managed();
        let enrollment_store = EnrollmentStore::new(&enrollment).unwrap();
        enrollment_store.create(&managed).unwrap();
        let catalog_store = CatalogStore::new(&catalog).unwrap();
        catalog_store
            .create_base(
                managed_key(),
                managed.base_inodes(),
                managed.identity().clone(),
                security_proof(),
            )
            .unwrap();
        Self {
            _root: root,
            enrollment,
            catalog,
            rescue,
            ordinary,
            managed,
        }
    }

    pub(crate) const fn managed(&self) -> &ManagedPackage {
        &self.managed
    }

    pub(crate) fn key() -> PackageKey {
        managed_key()
    }

    pub(crate) fn backend(&self) -> FakeRescueBackend {
        FakeRescueBackend::preview(&self.managed)
    }

    pub(crate) fn platform<F: RescueFaultInjector>(
        &self,
        backend: FakeRescueBackend,
        metadata: FakeMetadata,
        faults: F,
    ) -> OfflineRescuePlatform<FakeRescueBackend, FakeMetadata, F> {
        OfflineRescuePlatform::with_dependencies(
            backend,
            metadata,
            faults,
            &self.enrollment,
            &self.catalog,
            &self.rescue,
        )
    }

    pub(crate) fn journal(&self) -> RescueJournalStore {
        RescueJournalStore::for_package(&self.rescue, self.managed.package_name()).unwrap()
    }

    pub(crate) fn corrupt_ordinary_stores(&self) {
        let registry_root = self.ordinary.join("registry");
        let registry = RegistryStore::new(&registry_root).unwrap();
        let revisions = registry_root.join("packages/com.uclone.slotprobe/revisions");
        fs::create_dir_all(&revisions).unwrap();
        fs::write(revisions.join("0000000000000001.json"), b"{not-json\n").unwrap();
        assert!(registry.latest(self.managed.package_name()).is_err());

        let journal_root = self.ordinary.join("journal");
        let journal = JournalStore::new(&journal_root).unwrap();
        fs::create_dir_all(journal_root.join("transactions/corrupt-tx")).unwrap();
        assert!(journal.list().is_err());

        let state_root = self.ordinary.join("package-state");
        let states = PackageStateStore::new(&state_root).unwrap();
        states.initialize(&Self::key()).unwrap();
        fs::write(
            states.revision_path(self.managed.package_name(), 1),
            b"{not-json\n",
        )
        .unwrap();
        assert!(states.latest(&Self::key()).is_err());
    }

    pub(crate) fn ordinary_root(&self) -> &Path {
        &self.ordinary
    }
}

pub(crate) fn identity(uid: u32, signature: &str, version: u64) -> AppIdentity {
    AppIdentity::new(uid, signature, version, "/data/app/slotprobe/base.apk").unwrap()
}

pub(crate) fn base_inodes() -> DataInodes {
    DataInodes::new(101, 201).unwrap()
}

fn managed() -> ManagedPackage {
    let base = base_inodes();
    ManagedPackage::new(
        managed_key(),
        identity(10_321, SIGNATURE, 1),
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap()
}

fn managed_key() -> PackageKey {
    PackageKey::new(
        PackageName::parse("com.uclone.slotprobe").unwrap(),
        UserId::PRIMARY,
    )
}

fn security_proof() -> SecurityProfileProof {
    let path = PathSecurityProof::new(
        10_321,
        10_321,
        0o700,
        "u:object_r:app_data_file:s0:c1,c2",
        POLICY,
    )
    .unwrap();
    SecurityProfileProof::new(path.clone(), path)
}
