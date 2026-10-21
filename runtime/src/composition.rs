use std::path::Path;
use std::sync::Arc;

use thiserror::Error;

use crate::adapters::{FilePackageStore, FileSlotStorage, SystemAndroidOps};
use crate::launch::LaunchGate;
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

    pub fn prepare_default_account(
        &mut self,
        package: &str,
        still_foreground: impl FnMut() -> bool,
    ) -> Result<bool, RuntimeError> {
        let package = crate::model::PackageName::new(package)
            .map_err(|_error| RuntimeError::InvalidRequest)?;
        self.inner
            .prepare_default_account(&package, still_foreground)
    }

    pub(crate) fn inner_mut(&mut self) -> &mut InnerRuntime {
        &mut self.inner
    }
}

pub fn production(
    runtime_root: &Path,
    build_id: impl Into<String>,
) -> Result<ProductionRuntime, CompositionError> {
    compose(runtime_root, build_id, None)
}

pub fn production_with_launch_gate(
    runtime_root: &Path,
    build_id: impl Into<String>,
    gate: Arc<LaunchGate>,
) -> Result<ProductionRuntime, CompositionError> {
    compose(runtime_root, build_id, Some(gate))
}

fn compose(
    runtime_root: &Path,
    build_id: impl Into<String>,
    gate: Option<Arc<LaunchGate>>,
) -> Result<ProductionRuntime, CompositionError> {
    let packages = FilePackageStore::open(runtime_root)?;
    let slots = FileSlotStorage::open(
        layout::CE_SLOTS_ROOT,
        layout::DE_SLOTS_ROOT,
        layout::USER_CE_ROOT,
        layout::USER_DE_ROOT,
    )?;
    let mut android = SystemAndroidOps::new(
        build_id,
        layout::CE_SLOTS_ROOT,
        layout::DE_SLOTS_ROOT,
        layout::USER_CE_ROOT,
        layout::USER_DE_ROOT,
    );
    android.set_launch_gate(runtime_root, gate);
    Ok(ProductionRuntime {
        inner: Runtime::new(packages, slots, android),
    })
}
