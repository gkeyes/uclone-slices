#![doc = "Command-based Android adapter for the fail-closed slot runtime."]

mod backend;
mod base_rescue;
mod command;
mod containment;
mod gate_restore;
mod gate_snapshot;
mod launch;
mod lease;
mod materializer;
mod package_observation;
mod policy;
mod probe;
mod quiesce;
mod recovery;
pub mod system;
mod transition;
mod view;

pub use backend::AndroidBackend;
pub use command::{AndroidCommand, CommandError, CommandKind, CommandRunner, DataDomain};
pub use lease::{
    EmergencyGateLease, EmergencyGatePhase, FileGateLeaseStore, GateLease, GateLeaseError,
    GateLeaseStore, StoredGateLease,
};
pub use materializer::{
    AndroidMaterializer, MaterializerCommand, MaterializerCommandKind, MaterializerExecError,
    MaterializerExecutor, MaterializerLimits, MaterializerOutput, SystemMaterializerExecutor,
};
pub use probe::{
    CanonicalView, MountCounts, MountNamespaceProof, PackageProbe, ProbeError, ViewProof,
};
pub use system::{
    FactError, ProcessExecutor, ProcessFailure, ProcessInvocation, ProcessOutput, ProcessSet,
    StdProcessExecutor, StdSystemFacts, SystemCommandRunner, SystemFacts, SystemPackageProbe,
};
