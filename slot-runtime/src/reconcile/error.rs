use crate::domain::{PackageName, TransactionId};
use crate::enrollment::EnrollmentError;
use crate::journal::JournalError;
use crate::runtime::PlatformError;

#[doc = "Failures that prevent a complete reconciliation report."]
#[derive(Debug, thiserror::Error)]
pub enum ReconcileError {
    #[doc = "Enrollment enumeration could not identify every managed package."]
    #[error(transparent)]
    Enrollment(#[from] EnrollmentError),
    #[doc = "The compiled Preview allowlist entry is not a valid package name."]
    #[error("invalid compiled Preview package allowlist")]
    InvalidAllowlistConfiguration,
    #[doc = "An allowlisted package could not be durably held before metadata validation."]
    #[error("emergency-gate package {package}: {source}")]
    EmergencyGate {
        #[doc = "The recognizable package that could not be held."]
        package: PackageName,
        #[doc = "The platform gate failure."]
        #[source]
        source: PlatformError,
    },
    #[doc = "No captured exact gate state remained for a validated enrollment."]
    #[error("missing emergency gate snapshot for {0}")]
    MissingEmergencySnapshot(PackageName),
    #[doc = "A sentinel emergency lease could not be upgraded with enrollment anchors."]
    #[error("confirm emergency gate for {package}: {source}")]
    ConfirmEmergencyGate {
        #[doc = "The validated enrolled package."]
        package: PackageName,
        #[doc = "The platform lease confirmation failure."]
        #[source]
        source: PlatformError,
    },
    #[doc = "A repeated hold did not return the original exact gate snapshot."]
    #[error("emergency gate snapshot changed for {0}")]
    GateSnapshotMismatch(PackageName),
    #[doc = "The runtime could not prove the package disabled with no surviving processes."]
    #[error("contain package {package}: {source}")]
    Containment {
        #[doc = "The package whose execution gate could not be proved."]
        package: PackageName,
        #[doc = "The platform containment failure."]
        #[source]
        source: PlatformError,
    },
    #[doc = "A legal `RecoveryRequired` marker could not be made durable."]
    #[error("record recovery for transaction {transaction_id}: {source}")]
    RecoveryMarker {
        #[doc = "The unfinished transaction requiring recovery."]
        transaction_id: TransactionId,
        #[doc = "The durable journal failure."]
        #[source]
        source: JournalError,
    },
    #[doc = "The unlock phase was invoked without a successful early-boot hold phase."]
    #[error("early-boot package hold must run before unlocked reconciliation")]
    EarlyBootRequired,
    #[doc = "The platform could not determine user0 unlock state."]
    #[error("determine user0 unlock state: {0}")]
    UnlockState(PlatformError),
}
