use serde::{Deserialize, Serialize};

#[doc = "Durable enrollment-attempt phase."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnrollmentAttemptPhase {
    #[doc = "Gate was captured and enrollment has not yet been published."]
    Pending,
    #[doc = "A publication or cleanup boundary is ambiguous and needs recovery."]
    RecoveryRequired,
    #[doc = "All enrollment stores were published and the commit marker is durable."]
    Committed,
}
