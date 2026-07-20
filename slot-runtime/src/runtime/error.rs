use crate::journal::JournalError;
use crate::lifecycle::GuardDecision;
use crate::registry::RegistryError;

use super::{FaultPoint, RecoveryCause};

#[doc = "Typed platform operation failure returned by a runtime backend."]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlatformError {
    #[doc = "A coherent package observation could not be produced."]
    #[error("observe package failed: {detail}")]
    ObservePackage {
        #[doc = "Backend-specific stable failure detail."]
        detail: String,
    },
    #[doc = "The original package gate state could not be captured."]
    #[error("capture gate snapshot failed: {detail}")]
    CaptureGateSnapshot {
        #[doc = "Backend-specific stable failure detail."]
        detail: String,
    },
    #[doc = "The package execution gate could not be acquired."]
    #[error("acquire execution gate failed: {detail}")]
    AcquireGate {
        #[doc = "Backend-specific stable failure detail."]
        detail: String,
    },
    #[doc = "The held package execution gate could not be verified."]
    #[error("verify execution gate failed: {detail}")]
    VerifyGateHeld {
        #[doc = "Backend-specific stable failure detail."]
        detail: String,
    },
    #[doc = "Package processes could not be fully quiesced."]
    #[error("quiesce package processes failed: {detail}")]
    QuiesceProcesses {
        #[doc = "Backend-specific stable failure detail."]
        detail: String,
    },
    #[doc = "The paired CE and DE view could not be applied."]
    #[error("apply slot view failed: {detail}")]
    ApplySlotView {
        #[doc = "Backend-specific stable failure detail."]
        detail: String,
    },
    #[doc = "The requested slot view could not be proved."]
    #[error("verify slot view failed: {detail}")]
    VerifySlotView {
        #[doc = "Backend-specific stable failure detail."]
        detail: String,
    },
    #[doc = "The exact original package gate state could not be restored."]
    #[error("restore execution gate failed: {detail}")]
    RestoreGate {
        #[doc = "Backend-specific stable failure detail."]
        detail: String,
    },
    #[doc = "The durable recovery lease could not be retired after gate release was recorded."]
    #[error("retire execution-gate lease failed: {detail}")]
    RetireGateLease {
        #[doc = "Backend-specific stable failure detail."]
        detail: String,
    },
}

impl PlatformError {
    #[doc = "Builds a view-verification failure with a backend-specific detail code."]
    pub fn view_verification(detail: &str) -> Self {
        Self::VerifySlotView {
            detail: detail.to_owned(),
        }
    }
}

#[doc = "Failures that prevent a coordinator from producing a normal switch outcome."]
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[doc = "The package lifecycle guard rejected the request before mutation."]
    #[error("package lifecycle guard rejected switch: {0:?}")]
    GuardRejected(GuardDecision),
    #[doc = "A platform failure happened before a durable transaction could own recovery."]
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[doc = "A durable journal operation failed before platform mutation."]
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[doc = "A Registry operation failed before it became a recovery outcome."]
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[doc = "The runtime could not prove that the recovery execution gate is held."]
    #[error("recovery containment failed after {cause}: {containment}")]
    ContainmentFailed {
        #[doc = "The failure that first required fail-closed recovery."]
        cause: RecoveryCause,
        #[doc = "The platform failure that prevented gate containment proof."]
        containment: PlatformError,
    },
    #[doc = "The gate is held, but the recovery-required marker could not be made durable."]
    #[error("persist recovery-required marker failed after {cause}: {source}")]
    RecoveryMarkerFailed {
        #[doc = "The failure that first required fail-closed recovery."]
        cause: RecoveryCause,
        #[doc = "The Journal failure that prevented a durable recovery marker."]
        source: JournalError,
    },
    #[doc = "A deterministic test crash stopped execution without cleanup."]
    #[error("injected crash after {0:?}")]
    InjectedCrash(FaultPoint),
}
