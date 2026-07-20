#![doc = "Fail-closed CE/DE data-slot materialization core."]

mod backend;
mod coordinator;
mod error;
mod fault;
mod model;
mod validation;

pub use backend::MaterializationBackend;
pub use coordinator::MaterializationCoordinator;
pub use error::{
    BackendFailure, MaterializationError, MaterializationProofError, MaterializationStage,
};
pub use fault::{FaultInjector, FaultPoint, NoFault, ScriptedFaultInjector};
pub use model::{
    ArtifactState, BaseAnchor, ContentDigest, ContentProof, DataBytes, DataDomain, DirectoryAnchor,
    DomainCopyProof, MaterializationPaths, MaterializationResult, SlotMaterializationProof,
    TreeSafetyProof, UnsafeArtifact,
};
