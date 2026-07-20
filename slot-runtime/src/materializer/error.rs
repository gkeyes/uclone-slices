use crate::domain::SlotId;

use super::{DataDomain, FaultPoint, UnsafeArtifact};

#[doc = "Invalid typed filesystem proof construction."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MaterializationProofError {
    #[doc = "A content digest is not exactly 32 hexadecimal bytes."]
    #[error("invalid content SHA-256 digest")]
    InvalidDigest,
    #[doc = "A filesystem device identifier is zero."]
    #[error("invalid zero filesystem device identifier")]
    InvalidDevice,
    #[doc = "Directory anchors disagree with the paired content proof."]
    #[error("directory anchors disagree with paired content proof")]
    InconsistentContent,
    #[doc = "CE and DE claim the same directory identity on one filesystem."]
    #[error("CE and DE reuse one directory identity")]
    DuplicateDirectoryIdentity,
}

#[doc = "A stable materialization platform failure without a shell command payload."]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("materialization backend failure: {code}")]
pub struct BackendFailure {
    code: String,
}

impl BackendFailure {
    #[doc = "Constructs a backend failure from a bounded diagnostic code."]
    pub fn new(code: &str) -> Self {
        let code = if code.is_empty()
            || code.len() > 128
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            "invalid_backend_failure_code"
        } else {
            code
        };
        Self {
            code: code.to_owned(),
        }
    }

    #[doc = "Returns the non-command diagnostic code."]
    pub fn code(&self) -> &str {
        &self.code
    }
}

#[doc = "Security-sensitive materialization boundary used in typed diagnostics."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializationStage {
    #[doc = "Execution-gate verification."]
    Gate,
    #[doc = "Process quiescence verification."]
    Quiesce,
    #[doc = "Base inode and content anchoring."]
    BaseAnchor,
    #[doc = "Free-space capacity preflight."]
    Capacity,
    #[doc = "Staging and ready artifact inspection."]
    InspectArtifacts,
    #[doc = "Staging creation."]
    CreateStaging,
    #[doc = "CE or DE content copy."]
    Copy(DataDomain),
    #[doc = "Security metadata application."]
    ApplySecurity,
    #[doc = "Staging evidence collection."]
    InspectStaging,
    #[doc = "CE or DE durability synchronization."]
    Sync(DataDomain),
    #[doc = "Atomic ready publication."]
    Publish,
}

#[doc = "A fail-closed slot materialization rejection or platform failure."]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MaterializationError {
    #[doc = "Only the fixed first Preview slot is accepted."]
    #[error("unsupported materialization target slot: {0}")]
    UnsupportedSlot(SlotId),
    #[doc = "The immutable base view must be the active source."]
    #[error("materialization source is not the immutable base view")]
    SourceNotBase,
    #[doc = "A complete ready slot already exists and is never overwritten."]
    #[error("preview ready slot already exists")]
    ReadyAlreadyExists,
    #[doc = "Ready and staging artifacts coexist and require explicit recovery."]
    #[error("ambiguous staging and ready artifacts")]
    AmbiguousArtifacts,
    #[doc = "A platform boundary failed."]
    #[error("backend failed during {stage:?}: {source}")]
    Backend {
        #[doc = "The failed materialization stage."]
        stage: MaterializationStage,
        #[doc = "The backend diagnostic."]
        #[source]
        source: BackendFailure,
    },
    #[doc = "The observed base inode pair differs from enrollment."]
    #[error("base inode anchor differs from enrollment")]
    BaseAnchorMismatch,
    #[doc = "The base inode, content, filesystem, or security anchor changed during copying."]
    #[error("immutable base anchor changed during materialization")]
    BaseChanged,
    #[doc = "A copied tree contains a forbidden filesystem artifact."]
    #[error("{domain:?} copied tree contains forbidden {artifact:?}")]
    UnsafeTree {
        #[doc = "The affected data domain."]
        domain: DataDomain,
        #[doc = "The forbidden artifact kind."]
        artifact: UnsafeArtifact,
    },
    #[doc = "Copied content does not equal its base content anchor."]
    #[error("{0:?} copied content digest differs from base")]
    ContentMismatch(DataDomain),
    #[doc = "The staging filesystem device differs from the corresponding base device."]
    #[error("{0:?} staging filesystem device differs from base")]
    DeviceMismatch(DataDomain),
    #[doc = "UID, GID, mode, `SELinux` type, or MCS categories differ from base."]
    #[error("{0:?} staging security metadata differs from base")]
    SecurityMismatch(DataDomain),
    #[doc = "The staging fscrypt policy proof differs from base."]
    #[error("{0:?} staging fscrypt policy differs from base")]
    FscryptMismatch(DataDomain),
    #[doc = "CE and DE staging did not form one complete independent pair."]
    #[error("staging CE/DE pair is incomplete or reuses base inodes")]
    StagingIncomplete,
    #[doc = "A backend returned a copy receipt for the wrong data domain."]
    #[error("copy receipt domain differs from requested {0:?} domain")]
    CopyProofDomainMismatch(DataDomain),
    #[doc = "The final publication proof differs from the verified staging proof."]
    #[error("published ready proof differs from verified staging proof")]
    PublicationMismatch,
    #[doc = "A deterministic test fault interrupted one exact boundary."]
    #[error("fault injected after {0:?}")]
    FaultInjected(FaultPoint),
    #[doc = "Cleanup failed after an incomplete operation; gate must remain held."]
    #[error("cleanup failed after {original}: {cleanup}")]
    CleanupFailed {
        #[doc = "The original materialization failure."]
        original: String,
        #[doc = "The cleanup backend failure."]
        cleanup: BackendFailure,
    },
}

impl MaterializationError {
    #[doc = "Returns a stable high-level classification for status reporting."]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::SecurityMismatch(_) => "security",
            Self::FscryptMismatch(_) => "fscrypt",
            Self::DeviceMismatch(_) => "device",
            Self::BaseAnchorMismatch | Self::BaseChanged => "base",
            Self::UnsupportedSlot(_) => "slot",
            Self::SourceNotBase => "source",
            Self::ReadyAlreadyExists => "duplicate",
            Self::AmbiguousArtifacts => "artifacts",
            Self::Backend { .. } => "backend",
            Self::UnsafeTree { .. } => "tree",
            Self::ContentMismatch(_) => "content",
            Self::StagingIncomplete => "staging",
            Self::CopyProofDomainMismatch(_) => "copy",
            Self::PublicationMismatch => "publication",
            Self::FaultInjected(_) => "fault",
            Self::CleanupFailed { .. } => "cleanup",
        }
    }
}

pub(super) const fn backend_failure(
    stage: MaterializationStage,
    source: BackendFailure,
) -> MaterializationError {
    MaterializationError::Backend { stage, source }
}
