#![doc = "Fixed-layout, fail-closed Android slot materialization adapter."]

mod backend;
mod capacity;
mod command;
mod evidence;
mod executor;
mod fsops;
mod helpers;
mod inspection;
mod limits;
mod policy;
mod tree;

pub use backend::AndroidMaterializer;
pub use command::{MaterializerCommand, MaterializerCommandKind};
pub use executor::{
    MaterializerExecError, MaterializerExecutor, MaterializerOutput, SystemMaterializerExecutor,
};
pub use limits::MaterializerLimits;
