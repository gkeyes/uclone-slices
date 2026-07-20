use serde::{Deserialize, Serialize};

use super::{PackageStateError, PackageStateReason};
use crate::domain::{PackageKey, PackageName, UserId};
use crate::integrity::digest_json;
use crate::lifecycle::LifecycleState;

pub(super) const SCHEMA_VERSION: u32 = 1;

#[doc = "One immutable, hash-linked package lifecycle-state revision."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageStateRevision {
    pub(super) schema_version: u32,
    pub(super) generation: u64,
    pub(super) package_name: PackageName,
    pub(super) user_id: UserId,
    pub(super) lifecycle_state: LifecycleState,
    pub(super) reason: PackageStateReason,
    pub(super) previous_sha256: Option<String>,
    pub(super) sha256: String,
}

impl PackageStateRevision {
    #[doc = "Returns the persisted package-state schema version."]
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[doc = "Returns the package key carried by this revision."]
    pub fn package_key(&self) -> PackageKey {
        PackageKey::new(self.package_name.clone(), self.user_id)
    }

    #[doc = "Returns the package name."]
    pub const fn package_name(&self) -> &PackageName {
        &self.package_name
    }

    #[doc = "Returns the Android user id."]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    #[doc = "Returns the one-based stream generation."]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[doc = "Returns the persisted lifecycle state."]
    pub const fn lifecycle_state(&self) -> LifecycleState {
        self.lifecycle_state
    }

    #[doc = "Returns the typed reason for this state revision."]
    pub const fn reason(&self) -> PackageStateReason {
        self.reason
    }

    #[doc = "Returns the previous revision digest, when present."]
    pub fn previous_sha256(&self) -> Option<&str> {
        self.previous_sha256.as_deref()
    }

    #[doc = "Returns this revision's lowercase SHA-256 digest."]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(super) fn initialized(key: &PackageKey) -> Result<Self, PackageStateError> {
        Self::new(
            key,
            1,
            None,
            LifecycleState::Normal,
            PackageStateReason::Enrolled,
        )
    }

    pub(super) fn transitioned(
        previous: &Self,
        next: LifecycleState,
        reason: PackageStateReason,
    ) -> Result<Self, PackageStateError> {
        if !reason_matches_state(next, reason, false) {
            return Err(PackageStateError::Corrupt(
                "lifecycle state and reason do not match".to_owned(),
            ));
        }
        let generation = previous
            .generation
            .checked_add(1)
            .ok_or_else(|| PackageStateError::Corrupt("generation overflow".to_owned()))?;
        Self::new(
            &previous.package_key(),
            generation,
            Some(previous.sha256.clone()),
            next,
            reason,
        )
    }

    pub(super) fn verify(&self, previous: Option<&Self>) -> Result<(), PackageStateError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(PackageStateError::Corrupt(
                "unsupported schema version".to_owned(),
            ));
        }
        if self.generation == 0 {
            return Err(PackageStateError::Corrupt("zero generation".to_owned()));
        }
        let expected_generation = previous.map_or(Ok(1), |value| {
            value
                .generation
                .checked_add(1)
                .ok_or_else(|| PackageStateError::Corrupt("generation overflow".to_owned()))
        })?;
        if self.generation != expected_generation
            || self.previous_sha256.as_deref() != previous.map(Self::sha256)
        {
            return Err(PackageStateError::Corrupt(
                "generation or previous digest mismatch".to_owned(),
            ));
        }
        if self.digest()? != self.sha256 {
            return Err(PackageStateError::Corrupt("digest mismatch".to_owned()));
        }
        if !reason_matches_state(self.lifecycle_state, self.reason, previous.is_none()) {
            return Err(PackageStateError::Corrupt(
                "lifecycle state and reason do not match".to_owned(),
            ));
        }
        if let Some(previous) = previous {
            if previous.package_name != self.package_name || previous.user_id != self.user_id {
                return Err(PackageStateError::Corrupt(
                    "revision changes package stream identity".to_owned(),
                ));
            }
            if !previous
                .lifecycle_state
                .can_transition_to(self.lifecycle_state)
            {
                return Err(PackageStateError::Corrupt(format!(
                    "illegal lifecycle transition from {:?} to {:?}",
                    previous.lifecycle_state, self.lifecycle_state
                )));
            }
        }
        Ok(())
    }

    fn new(
        key: &PackageKey,
        generation: u64,
        previous_sha256: Option<String>,
        lifecycle_state: LifecycleState,
        reason: PackageStateReason,
    ) -> Result<Self, PackageStateError> {
        let mut revision = Self {
            schema_version: SCHEMA_VERSION,
            generation,
            package_name: key.package_name().clone(),
            user_id: key.user_id(),
            lifecycle_state,
            reason,
            previous_sha256,
            sha256: String::new(),
        };
        revision.sha256 = revision.digest()?;
        Ok(revision)
    }

    fn digest(&self) -> Result<String, PackageStateError> {
        Ok(digest_json(&UnsignedRevision::from(self))?)
    }
}

fn reason_matches_state(state: LifecycleState, reason: PackageStateReason, first: bool) -> bool {
    if first {
        return state == LifecycleState::Normal && reason == PackageStateReason::Enrolled;
    }
    match reason {
        PackageStateReason::Enrolled => false,
        PackageStateReason::ManagedUpdate => matches!(
            state,
            LifecycleState::UpdatePreparing
                | LifecycleState::UpdateWindowOpen
                | LifecycleState::UpdateVerifying
        ),
        PackageStateReason::IdentityChanged => state == LifecycleState::Quarantined,
        PackageStateReason::ViewUncertain => state == LifecycleState::RecoveryRequired,
        PackageStateReason::LifecycleDrift => {
            matches!(
                state,
                LifecycleState::LifecycleDrifted | LifecycleState::RecoveryRequired
            )
        }
        PackageStateReason::ManualRepair => {
            matches!(
                state,
                LifecycleState::RepairWaiting | LifecycleState::Normal
            )
        }
    }
}

#[derive(Serialize)]
struct UnsignedRevision<'a> {
    schema_version: u32,
    generation: u64,
    package_name: &'a PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    previous_sha256: Option<&'a str>,
}

impl<'a> From<&'a PackageStateRevision> for UnsignedRevision<'a> {
    fn from(value: &'a PackageStateRevision) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            generation: value.generation,
            package_name: &value.package_name,
            user_id: value.user_id,
            lifecycle_state: value.lifecycle_state,
            reason: value.reason,
            previous_sha256: value.previous_sha256.as_deref(),
        }
    }
}
