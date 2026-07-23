use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::domain::{InstalledArtifact, ManagedUpdateContext, PackageName, UserId};
use crate::integrity::digest_json;
use crate::lifecycle::LifecycleState;

use super::{PackageStateRevision, V1_SCHEMA_VERSION, V2_SCHEMA_VERSION};
use crate::package_state::{PackageStateError, PackageStateReason};

#[derive(Deserialize)]
#[serde(untagged)]
enum RevisionWire {
    V1(RevisionV1),
    V2(RevisionV2),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionV1 {
    schema_version: u32,
    generation: u64,
    package_name: PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    previous_sha256: Option<String>,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionV2 {
    schema_version: u32,
    generation: u64,
    package_name: PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    accepted_artifact: InstalledArtifact,
    managed_update_context: Option<ManagedUpdateContext>,
    previous_sha256: Option<String>,
    sha256: String,
}

impl<'de> Deserialize<'de> for PackageStateRevision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match RevisionWire::deserialize(deserializer)? {
            RevisionWire::V1(value) if value.schema_version == V1_SCHEMA_VERSION => Ok(Self {
                schema_version: value.schema_version,
                generation: value.generation,
                package_name: value.package_name,
                user_id: value.user_id,
                lifecycle_state: value.lifecycle_state,
                reason: value.reason,
                accepted_artifact: None,
                managed_update_context: None,
                previous_sha256: value.previous_sha256,
                sha256: value.sha256,
            }),
            RevisionWire::V2(value) if value.schema_version == V2_SCHEMA_VERSION => Ok(Self {
                schema_version: value.schema_version,
                generation: value.generation,
                package_name: value.package_name,
                user_id: value.user_id,
                lifecycle_state: value.lifecycle_state,
                reason: value.reason,
                accepted_artifact: Some(value.accepted_artifact),
                managed_update_context: value.managed_update_context,
                previous_sha256: value.previous_sha256,
                sha256: value.sha256,
            }),
            _ => Err(serde::de::Error::custom(
                "unsupported package state schema version",
            )),
        }
    }
}

impl Serialize for PackageStateRevision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self.schema_version {
            V1_SCHEMA_VERSION => SignedV1::from(self).serialize(serializer),
            V2_SCHEMA_VERSION => SignedV2::try_from(self)
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer),
            _ => Err(serde::ser::Error::custom(
                "unsupported package state schema",
            )),
        }
    }
}

pub(super) fn digest_v1(value: &PackageStateRevision) -> Result<String, PackageStateError> {
    Ok(digest_json(&UnsignedV1::from(value))?)
}

pub(super) fn digest_v2(value: &PackageStateRevision) -> Result<String, PackageStateError> {
    Ok(digest_json(&UnsignedV2::try_from(value)?)?)
}

#[derive(Serialize)]
struct SignedV1<'a> {
    schema_version: u32,
    generation: u64,
    package_name: &'a PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    previous_sha256: Option<&'a str>,
    sha256: &'a str,
}

impl<'a> From<&'a PackageStateRevision> for SignedV1<'a> {
    fn from(value: &'a PackageStateRevision) -> Self {
        Self {
            schema_version: V1_SCHEMA_VERSION,
            generation: value.generation,
            package_name: &value.package_name,
            user_id: value.user_id,
            lifecycle_state: value.lifecycle_state,
            reason: value.reason,
            previous_sha256: value.previous_sha256.as_deref(),
            sha256: &value.sha256,
        }
    }
}

#[derive(Serialize)]
struct SignedV2<'a> {
    schema_version: u32,
    generation: u64,
    package_name: &'a PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    accepted_artifact: &'a InstalledArtifact,
    managed_update_context: Option<&'a ManagedUpdateContext>,
    previous_sha256: Option<&'a str>,
    sha256: &'a str,
}

impl<'a> TryFrom<&'a PackageStateRevision> for SignedV2<'a> {
    type Error = PackageStateError;

    fn try_from(value: &'a PackageStateRevision) -> Result<Self, Self::Error> {
        Ok(Self {
            schema_version: V2_SCHEMA_VERSION,
            generation: value.generation,
            package_name: &value.package_name,
            user_id: value.user_id,
            lifecycle_state: value.lifecycle_state,
            reason: value.reason,
            accepted_artifact: value.accepted_artifact.as_ref().ok_or_else(|| {
                PackageStateError::Corrupt("v2 revision is missing accepted artifact".to_owned())
            })?,
            managed_update_context: value.managed_update_context.as_ref(),
            previous_sha256: value.previous_sha256.as_deref(),
            sha256: &value.sha256,
        })
    }
}

#[derive(Serialize)]
struct UnsignedV1<'a> {
    schema_version: u32,
    generation: u64,
    package_name: &'a PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    previous_sha256: Option<&'a str>,
}

impl<'a> From<&'a PackageStateRevision> for UnsignedV1<'a> {
    fn from(value: &'a PackageStateRevision) -> Self {
        Self {
            schema_version: V1_SCHEMA_VERSION,
            generation: value.generation,
            package_name: &value.package_name,
            user_id: value.user_id,
            lifecycle_state: value.lifecycle_state,
            reason: value.reason,
            previous_sha256: value.previous_sha256.as_deref(),
        }
    }
}

#[derive(Serialize)]
struct UnsignedV2<'a> {
    schema_version: u32,
    generation: u64,
    package_name: &'a PackageName,
    user_id: UserId,
    lifecycle_state: LifecycleState,
    reason: PackageStateReason,
    accepted_artifact: &'a InstalledArtifact,
    managed_update_context: Option<&'a ManagedUpdateContext>,
    previous_sha256: Option<&'a str>,
}

impl<'a> TryFrom<&'a PackageStateRevision> for UnsignedV2<'a> {
    type Error = PackageStateError;

    fn try_from(value: &'a PackageStateRevision) -> Result<Self, Self::Error> {
        Ok(Self {
            schema_version: V2_SCHEMA_VERSION,
            generation: value.generation,
            package_name: &value.package_name,
            user_id: value.user_id,
            lifecycle_state: value.lifecycle_state,
            reason: value.reason,
            accepted_artifact: value.accepted_artifact.as_ref().ok_or_else(|| {
                PackageStateError::Corrupt("v2 revision is missing accepted artifact".to_owned())
            })?,
            managed_update_context: value.managed_update_context.as_ref(),
            previous_sha256: value.previous_sha256.as_deref(),
        })
    }
}
