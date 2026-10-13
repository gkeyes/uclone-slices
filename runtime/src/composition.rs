use std::path::Path;

use thiserror::Error;

use crate::adapters::{FilePackageStore, FileSlotStorage, SystemAndroidOps};
use crate::layout;
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
    pub fn reconcile_boot(&mut self) -> Result<(), RuntimeError> {
        self.inner.reconcile_boot()
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
