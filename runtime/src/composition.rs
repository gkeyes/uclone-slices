use std::path::Path;

use thiserror::Error;

use crate::adapters::{FilePackageStore, FileSlotStorage, SystemAndroidOps};
use crate::layout;
use crate::model::{
    Capabilities, DisplayName, PackageName, PackageSnapshot, SeedMode, SigningIdentity, SlotId,
};
use crate::ports::AdapterError;
use crate::usecases::{Runtime, RuntimeError};

type InnerRuntime = Runtime<FilePackageStore, FileSlotStorage, SystemAndroidOps>;

#[derive(Debug, Error)]
#[error("could not compose production Runtime: {message}")]
pub struct CompositionError {
    message: String,
}

impl From<AdapterError> for CompositionError {
    fn from(error: AdapterError) -> Self {
        Self {
            message: error.to_string(),
        }
    }
}

#[derive(Debug)]
pub struct ProductionRuntime {
    inner: InnerRuntime,
}

impl ProductionRuntime {
    pub fn probe(&mut self) -> Result<Capabilities, RuntimeError> {
        self.inner.probe()
    }

    pub fn list_packages(&mut self) -> Result<Vec<PackageSnapshot>, RuntimeError> {
        self.inner.list_packages()
    }

    pub fn reconcile_boot(&mut self) -> Result<(), RuntimeError> {
        self.inner.reconcile_boot()
    }

    pub fn get_package(&mut self, package: &PackageName) -> Result<PackageSnapshot, RuntimeError> {
        self.inner.get_package(package)
    }

    pub fn enroll(
        &mut self,
        package: PackageName,
        reset: bool,
        signing: Option<SigningIdentity>,
    ) -> Result<PackageSnapshot, RuntimeError> {
        self.inner.enroll(package, reset, signing)
    }

    pub fn rebind_package(
        &mut self,
        package: &PackageName,
        signing: SigningIdentity,
        trust_legacy: bool,
    ) -> Result<PackageSnapshot, RuntimeError> {
        self.inner.rebind_package(package, signing, trust_legacy)
    }

    pub fn unenroll(&mut self, package: &PackageName) -> Result<(), RuntimeError> {
        self.inner.unenroll(package)
    }

    pub fn create_slot(
        &mut self,
        package: &PackageName,
        display_name: DisplayName,
        seed: SeedMode,
    ) -> Result<PackageSnapshot, RuntimeError> {
        self.inner.create_slot(package, display_name, seed)
    }

    pub fn rename_slot(
        &mut self,
        package: &PackageName,
        target: &SlotId,
        display_name: DisplayName,
    ) -> Result<PackageSnapshot, RuntimeError> {
        self.inner.rename_slot(package, target, display_name)
    }

    pub fn delete_slot(
        &mut self,
        package: &PackageName,
        target: &SlotId,
    ) -> Result<PackageSnapshot, RuntimeError> {
        self.inner.delete_slot(package, target)
    }

    pub fn activate_slot(
        &mut self,
        package: &PackageName,
        target: &SlotId,
    ) -> Result<PackageSnapshot, RuntimeError> {
        self.inner.activate_slot(package, target)
    }

    pub(crate) fn inner_mut(&mut self) -> &mut InnerRuntime {
        &mut self.inner
    }
}

pub fn production(
    runtime_root: &Path,
    build_id: impl Into<String>,
) -> Result<ProductionRuntime, CompositionError> {
    let packages = FilePackageStore::open(runtime_root)?;
    let slots = FileSlotStorage::open(
        layout::CE_SLOTS_ROOT,
        layout::DE_SLOTS_ROOT,
        layout::USER_CE_ROOT,
        layout::USER_DE_ROOT,
    )?;
    let android = SystemAndroidOps::new(
        build_id,
        layout::CE_SLOTS_ROOT,
        layout::DE_SLOTS_ROOT,
        layout::USER_CE_ROOT,
        layout::USER_DE_ROOT,
    );
    Ok(ProductionRuntime {
        inner: Runtime::new(packages, slots, android),
    })
}
