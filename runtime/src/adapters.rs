mod android;
mod filesystem;
#[cfg(test)]
mod memory;

pub(crate) use android::SystemAndroidOps;
pub(crate) use filesystem::{FilePackageStore, FileSlotStorage};
#[cfg(test)]
pub(crate) use memory::{
    AndroidCall, AndroidFailure, MemoryAndroidOps, MemoryPackageStore, MemorySlotStorage,
    SlotFailure,
};
