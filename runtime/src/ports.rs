use thiserror::Error;

use crate::model::{
    Capabilities, ObservedView, PackageAggregate, PackageInspection, PackageName, SeedMode, Slot,
    SlotId,
};

#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{message}")]
pub(crate) struct AdapterError {
    state_conflict: bool,
    message: String,
}

impl AdapterError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            state_conflict: false,
            message: message.into(),
        }
    }

    pub(crate) fn state_conflict(message: impl Into<String>) -> Self {
        Self {
            state_conflict: true,
            message: message.into(),
        }
    }

    pub(crate) fn is_state_conflict(&self) -> bool {
        self.state_conflict
    }
}

pub(crate) trait PackageStore: core::fmt::Debug {
    fn list(&self) -> Result<Vec<PackageName>, AdapterError>;
    fn load(&self, package: &PackageName) -> Result<Option<PackageAggregate>, AdapterError>;
    fn save(&mut self, aggregate: &PackageAggregate) -> Result<(), AdapterError>;
    fn remove(&mut self, package: &PackageName) -> Result<(), AdapterError>;
}

pub(crate) trait SlotStorage: core::fmt::Debug {
    fn materialize(
        &mut self,
        package: &PackageName,
        slot: &Slot,
        seed: SeedMode,
    ) -> Result<(), AdapterError>;

    fn discard(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError>;

    fn discard_package(&mut self, package: &PackageName) -> Result<(), AdapterError>;

    fn require_complete_pair(
        &self,
        package: &PackageName,
        slot: &SlotId,
    ) -> Result<(), AdapterError>;
}

pub(crate) trait AndroidOps: core::fmt::Debug {
    fn probe(&mut self) -> Result<Capabilities, AdapterError>;
    fn inspect(&mut self, package: &PackageName) -> Result<PackageInspection, AdapterError>;
    fn force_stop(&mut self, package: &PackageName) -> Result<(), AdapterError>;
    fn observe_view(&mut self, package: &PackageName) -> Result<ObservedView, AdapterError>;
    fn apply_view(&mut self, package: &PackageName, slot: &SlotId) -> Result<(), AdapterError>;
    fn launch_verified(
        &mut self,
        package: &PackageName,
        expected: &SlotId,
    ) -> Result<(), AdapterError>;
}
