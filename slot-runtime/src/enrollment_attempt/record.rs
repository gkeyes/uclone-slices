use serde::{Deserialize, Serialize};

use crate::domain::{AppIdentity, DataInodes, GateSnapshot, PackageKey, UserId};
use crate::integrity::digest_json;

use super::super::EnrollmentAttemptError;
use super::SCHEMA_VERSION;
use super::anchors::CommittedAnchors;
use super::phase::EnrollmentAttemptPhase;

#[doc = "Hash-protected versioned enrollment attempt record."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentAttempt {
    pub(super) schema_version: u32,
    pub(super) generation: u64,
    pub(super) package_key: PackageKey,
    pub(super) gate_snapshot: GateSnapshot,
    pub(super) phase: EnrollmentAttemptPhase,
    pub(super) committed: Option<CommittedAnchors>,
    pub(super) previous_sha256: Option<String>,
    pub(super) sha256: String,
    #[serde(skip)]
    pub(super) authoritative: bool,
}

impl EnrollmentAttempt {
    #[doc = "Returns the package key protected by the attempt."]
    pub const fn package_key(&self) -> &PackageKey {
        &self.package_key
    }

    #[doc = "Returns the exact pre-enrollment gate snapshot."]
    pub const fn gate_snapshot(&self) -> GateSnapshot {
        self.gate_snapshot
    }

    #[doc = "Returns the effective phase; markerless commits are recovery-required."]
    pub fn phase(&self) -> EnrollmentAttemptPhase {
        if self.phase == EnrollmentAttemptPhase::Committed && !self.authoritative {
            EnrollmentAttemptPhase::RecoveryRequired
        } else {
            self.phase
        }
    }

    #[doc = "Returns the persisted generation number."]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[doc = "Returns committed identity and base anchors, when present."]
    pub const fn committed(&self) -> Option<&CommittedAnchors> {
        self.committed.as_ref()
    }

    pub(crate) const fn stored_phase(&self) -> EnrollmentAttemptPhase {
        self.phase
    }

    pub(crate) fn committed_anchors(&self) -> Option<CommittedAnchors> {
        self.committed.clone()
    }

    #[doc = "Returns the committed package digest, when present."]
    pub fn committed_managed_sha256(&self) -> Option<&str> {
        self.committed
            .as_ref()
            .map(CommittedAnchors::managed_sha256)
    }

    #[doc = "Returns the committed identity, when present."]
    pub fn committed_identity(&self) -> Option<&AppIdentity> {
        self.committed.as_ref().map(CommittedAnchors::identity)
    }

    #[doc = "Returns the committed immutable base anchors, when present."]
    pub fn committed_base_inodes(&self) -> Option<DataInodes> {
        self.committed.as_ref().map(CommittedAnchors::base_inodes)
    }

    #[doc = "Returns the durable record digest."]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[doc = "Returns whether a valid commit marker authorizes the commit."]
    pub const fn is_authoritative(&self) -> bool {
        self.authoritative
    }

    pub(crate) fn new(
        package_key: PackageKey,
        gate_snapshot: GateSnapshot,
        phase: EnrollmentAttemptPhase,
        committed: Option<CommittedAnchors>,
        generation: u64,
        previous_sha256: Option<String>,
    ) -> Result<Self, EnrollmentAttemptError> {
        let mut record = Self {
            schema_version: SCHEMA_VERSION,
            generation,
            package_key,
            gate_snapshot,
            phase,
            committed,
            previous_sha256,
            sha256: String::new(),
            authoritative: false,
        };
        record.sha256 = record.digest()?;
        Ok(record)
    }

    pub(crate) fn verify(&self, previous: Option<&Self>) -> Result<(), EnrollmentAttemptError> {
        if self.schema_version != SCHEMA_VERSION || self.generation == 0 {
            return Err(EnrollmentAttemptError::Corrupt(
                "unsupported schema or zero generation".to_owned(),
            ));
        }
        if self.package_key.user_id() != UserId::PRIMARY
            || self.package_key.package_name().as_str() != crate::protocol::ALLOWED_PACKAGE
        {
            return Err(EnrollmentAttemptError::Corrupt(
                "attempt package is outside the compiled user-zero allowlist".to_owned(),
            ));
        }
        let expected_generation = previous.map_or(Ok(1), |value| {
            value
                .generation
                .checked_add(1)
                .ok_or_else(|| EnrollmentAttemptError::Corrupt("generation overflow".to_owned()))
        })?;
        if self.generation != expected_generation
            || self.previous_sha256.as_deref() != previous.map(Self::sha256)
        {
            return Err(EnrollmentAttemptError::Corrupt(
                "generation or previous digest mismatch".to_owned(),
            ));
        }
        if self.digest()? != self.sha256 || !valid_sha256(&self.sha256) {
            return Err(EnrollmentAttemptError::Corrupt(
                "digest mismatch".to_owned(),
            ));
        }
        match self.phase {
            EnrollmentAttemptPhase::Pending if self.committed.is_some() => Err(
                EnrollmentAttemptError::Corrupt("pending record has committed anchors".to_owned()),
            ),
            EnrollmentAttemptPhase::Committed if self.committed.is_none() => Err(
                EnrollmentAttemptError::Corrupt("committed record has no anchors".to_owned()),
            ),
            _ => Ok(()),
        }?;
        if let Some(committed) = &self.committed {
            committed.validate()?;
        }
        Ok(())
    }

    pub(super) fn digest(&self) -> Result<String, EnrollmentAttemptError> {
        Ok(digest_json(&UnsignedAttempt::from(self))?)
    }

    pub(crate) const fn with_authority(mut self, authoritative: bool) -> Self {
        self.authoritative = authoritative;
        self
    }
}

#[derive(Serialize)]
struct UnsignedAttempt<'a> {
    schema_version: u32,
    generation: u64,
    package_key: &'a PackageKey,
    gate_snapshot: GateSnapshot,
    phase: EnrollmentAttemptPhase,
    committed: &'a Option<CommittedAnchors>,
    previous_sha256: Option<&'a str>,
}

impl<'a> From<&'a EnrollmentAttempt> for UnsignedAttempt<'a> {
    fn from(value: &'a EnrollmentAttempt) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            generation: value.generation,
            package_key: &value.package_key,
            gate_snapshot: value.gate_snapshot,
            phase: value.phase,
            committed: &value.committed,
            previous_sha256: value.previous_sha256.as_deref(),
        }
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
