use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use super::{DomainError, GateSnapshot, InstalledArtifact, PackageEnabledState};

#[doc = "Opaque, exact identity for one managed-update attempt."]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UpdateToken(String);

impl UpdateToken {
    #[doc = "Parses a token safe for exact durable equality checks."]
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        if (8..=64).contains(&raw.len())
            && raw.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
        {
            Ok(Self(raw.to_owned()))
        } else {
            Err(DomainError::InvalidUpdateToken)
        }
    }

    #[doc = "Returns the exact validated token."]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UpdateToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for UpdateToken {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<UpdateToken> for String {
    fn from(value: UpdateToken) -> Self {
        value.0
    }
}

#[doc = "Exact Gate and candidate facts retained during one managed update."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ManagedUpdateContext {
    token: UpdateToken,
    gate_snapshot: GateSnapshot,
    candidate_artifact: Option<InstalledArtifact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagedUpdateContextRecord {
    token: UpdateToken,
    gate_snapshot: GateSnapshotRecord,
    candidate_artifact: Option<InstalledArtifact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GateSnapshotRecord {
    enabled_state: PackageEnabledState,
    suspended: bool,
}

impl<'de> Deserialize<'de> for ManagedUpdateContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let record = ManagedUpdateContextRecord::deserialize(deserializer)?;
        Ok(Self {
            token: record.token,
            gate_snapshot: GateSnapshot::new(
                record.gate_snapshot.enabled_state,
                record.gate_snapshot.suspended,
            ),
            candidate_artifact: record.candidate_artifact,
        })
    }
}

impl ManagedUpdateContext {
    #[doc = "Captures an update token and exact pre-update Gate without a candidate."]
    pub const fn pending(token: UpdateToken, gate_snapshot: GateSnapshot) -> Self {
        Self {
            token,
            gate_snapshot,
            candidate_artifact: None,
        }
    }

    #[doc = "Captures the candidate observed inside the same exact update context."]
    pub const fn verifying(
        token: UpdateToken,
        gate_snapshot: GateSnapshot,
        candidate_artifact: InstalledArtifact,
    ) -> Self {
        Self {
            token,
            gate_snapshot,
            candidate_artifact: Some(candidate_artifact),
        }
    }

    #[doc = "Returns the exact update attempt token."]
    pub const fn token(&self) -> &UpdateToken {
        &self.token
    }

    #[doc = "Returns the Gate snapshot captured before this update."]
    pub const fn gate_snapshot(&self) -> GateSnapshot {
        self.gate_snapshot
    }

    #[doc = "Returns the observed candidate artifact, when verification has begun."]
    pub const fn candidate_artifact(&self) -> Option<&InstalledArtifact> {
        self.candidate_artifact.as_ref()
    }
}
