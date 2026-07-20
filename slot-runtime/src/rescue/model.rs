use std::fmt;

use serde::{Deserialize, Serialize};

use super::RescueError;
use crate::domain::{
    AppIdentity, BootId, CommitNonce, DataInodes, GateSnapshot, PackageKey, UserId,
};

mod implementation;

pub(super) const SCHEMA_VERSION: u32 = 1;

#[doc = "Validated identifier for one immutable emergency rescue epoch."]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RescueId(String);

impl RescueId {
    #[doc = "Parses an identifier that is safe for bounded durable records."]
    pub fn parse(raw: &str) -> Result<Self, RescueError> {
        if (8..=64).contains(&raw.len())
            && raw
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_lowercase())
            && raw.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
            })
        {
            Ok(Self(raw.to_owned()))
        } else {
            Err(RescueError::InvalidSpec("invalid rescue id".to_owned()))
        }
    }

    #[doc = "Returns the validated identifier."]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for RescueId {
    type Error = RescueError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<RescueId> for String {
    fn from(value: RescueId) -> Self {
        value.0
    }
}

impl fmt::Display for RescueId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[doc = "The sole emergency operation supported by the first Preview epoch."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RescueDisposition {
    #[doc = "Remove only proved Preview bind layers and retire Slots onto native base."]
    RetireToBase,
}

#[doc = "Immutable root-of-trust inputs captured before an emergency gate mutation."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RescueSpecRecord")]
pub struct RescueSpec {
    schema_version: u32,
    rescue_id: RescueId,
    package_key: PackageKey,
    enrolled_identity: AppIdentity,
    base_inodes: DataInodes,
    gate_snapshot: GateSnapshot,
    enrollment_sha256: String,
    base_manifest_sha256: String,
    boot_id: BootId,
    commit_nonce: CommitNonce,
    disposition: RescueDisposition,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RescueSpecRecord {
    schema_version: u32,
    rescue_id: RescueId,
    package_key: PackageKey,
    enrolled_identity: AppIdentity,
    base_inodes: DataInodes,
    gate_snapshot: GateSnapshot,
    enrollment_sha256: String,
    base_manifest_sha256: String,
    boot_id: BootId,
    commit_nonce: CommitNonce,
    disposition: RescueDisposition,
}

impl TryFrom<RescueSpecRecord> for RescueSpec {
    type Error = RescueError;

    fn try_from(value: RescueSpecRecord) -> Result<Self, Self::Error> {
        let spec = Self {
            schema_version: value.schema_version,
            rescue_id: value.rescue_id,
            package_key: value.package_key,
            enrolled_identity: value.enrolled_identity,
            base_inodes: value.base_inodes,
            gate_snapshot: value.gate_snapshot,
            enrollment_sha256: value.enrollment_sha256,
            base_manifest_sha256: value.base_manifest_sha256,
            boot_id: value.boot_id,
            commit_nonce: value.commit_nonce,
            disposition: value.disposition,
        };
        spec.validate()?;
        Ok(spec)
    }
}
