use crate::domain::{InstalledArtifact, ManagedUpdateContext, PackageKey, PackageName, UserId};
use crate::lifecycle::LifecycleState;

use super::{PackageStateError, PackageStateReason};

mod semantics;
mod validation;
mod wire;

pub(super) const V1_SCHEMA_VERSION: u32 = 1;
pub(super) const V2_SCHEMA_VERSION: u32 = 2;

#[doc = "One immutable, hash-linked package lifecycle-state revision."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageStateRevision {
    schema_version: u32,
    generation: u64,
    package_name: PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    accepted_artifact: Option<InstalledArtifact>,
    managed_update_context: Option<ManagedUpdateContext>,
    previous_sha256: Option<String>,
    sha256: String,
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

    #[doc = "Returns the v2 artifact accepted for ordinary execution."]
    pub const fn accepted_artifact(&self) -> Option<&InstalledArtifact> {
        self.accepted_artifact.as_ref()
    }

    #[doc = "Returns the exact v2 managed-update context, when executable."]
    pub const fn managed_update_context(&self) -> Option<&ManagedUpdateContext> {
        self.managed_update_context.as_ref()
    }

    #[doc = "Returns the previous revision digest, when present."]
    pub fn previous_sha256(&self) -> Option<&str> {
        self.previous_sha256.as_deref()
    }

    #[doc = "Returns this revision's lowercase SHA-256 digest."]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    pub(super) fn initialized_v1(key: &PackageKey) -> Result<Self, PackageStateError> {
        let revision = Self::build(
            V1_SCHEMA_VERSION,
            key,
            1,
            None,
            LifecycleState::Normal,
            PackageStateReason::Enrolled,
            None,
            None,
        )?;
        revision.verify(None)?;
        Ok(revision)
    }

    pub(super) fn migrated_normal_v2(
        previous: &Self,
        accepted_artifact: InstalledArtifact,
    ) -> Result<Self, PackageStateError> {
        let revision = Self::build_next(
            previous,
            V2_SCHEMA_VERSION,
            LifecycleState::Normal,
            PackageStateReason::Enrolled,
            Some(accepted_artifact),
            None,
        )?;
        revision.verify(Some(previous))?;
        Ok(revision)
    }

    pub(super) fn transitioned(
        previous: &Self,
        next: LifecycleState,
        reason: PackageStateReason,
    ) -> Result<Self, PackageStateError> {
        if semantics::is_update_state(previous.lifecycle_state) || semantics::is_update_state(next)
        {
            return Err(PackageStateError::Corrupt(
                "managed update requires a proof-bearing coordinator".to_owned(),
            ));
        }
        let (accepted_artifact, managed_update_context) = if previous.schema_version
            == V1_SCHEMA_VERSION
        {
            (None, None)
        } else {
            (
                Some(previous.accepted_artifact.clone().ok_or_else(|| {
                    PackageStateError::Corrupt("v2 head is missing accepted artifact".to_owned())
                })?),
                None,
            )
        };
        let revision = Self::build_next(
            previous,
            previous.schema_version,
            next,
            reason,
            accepted_artifact,
            managed_update_context,
        )?;
        revision.verify(Some(previous))?;
        Ok(revision)
    }

    pub(super) fn verify(&self, previous: Option<&Self>) -> Result<(), PackageStateError> {
        validation::verify(self, previous)
    }

    fn build_next(
        previous: &Self,
        schema_version: u32,
        lifecycle_state: LifecycleState,
        reason: PackageStateReason,
        accepted_artifact: Option<InstalledArtifact>,
        managed_update_context: Option<ManagedUpdateContext>,
    ) -> Result<Self, PackageStateError> {
        let generation = previous
            .generation
            .checked_add(1)
            .ok_or_else(|| PackageStateError::Corrupt("generation overflow".to_owned()))?;
        Self::build(
            schema_version,
            &previous.package_key(),
            generation,
            Some(previous.sha256.clone()),
            lifecycle_state,
            reason,
            accepted_artifact,
            managed_update_context,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        schema_version: u32,
        key: &PackageKey,
        generation: u64,
        previous_sha256: Option<String>,
        lifecycle_state: LifecycleState,
        reason: PackageStateReason,
        accepted_artifact: Option<InstalledArtifact>,
        managed_update_context: Option<ManagedUpdateContext>,
    ) -> Result<Self, PackageStateError> {
        let mut revision = Self {
            schema_version,
            generation,
            package_name: key.package_name().clone(),
            user_id: key.user_id(),
            lifecycle_state,
            reason,
            accepted_artifact,
            managed_update_context,
            previous_sha256,
            sha256: String::new(),
        };
        revision.sha256 = revision.digest()?;
        Ok(revision)
    }

    fn digest(&self) -> Result<String, PackageStateError> {
        match self.schema_version {
            V1_SCHEMA_VERSION => wire::digest_v1(self),
            V2_SCHEMA_VERSION => wire::digest_v2(self),
            _ => Err(PackageStateError::Corrupt(
                "unsupported schema version".to_owned(),
            )),
        }
    }
}
