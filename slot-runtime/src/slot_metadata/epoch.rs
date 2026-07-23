use serde::{Deserialize, Serialize};

use crate::domain::{PackageName, SlotId};
use crate::integrity::digest_json;

use super::{SlotMetadata, SlotMetadataError};

const EPOCH_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EpochCheckpoint {
    schema_version: u32,
    epoch: u64,
    package: PackageName,
    slot: SlotId,
    previous_epoch: Option<u64>,
    previous_terminal_sha256: String,
    previous_record_count: u64,
    canonical_record: SlotMetadata,
    sha256: String,
}

impl EpochCheckpoint {
    pub(super) fn new(
        epoch: u64,
        previous_epoch: Option<u64>,
        previous_record_count: usize,
        canonical_record: SlotMetadata,
    ) -> Result<Self, SlotMetadataError> {
        let previous_record_count = u64::try_from(previous_record_count)
            .map_err(|_| corrupt("slot metadata epoch record count overflow"))?;
        let mut value = Self {
            schema_version: EPOCH_SCHEMA_VERSION,
            epoch,
            package: canonical_record.package().clone(),
            slot: canonical_record.slot().clone(),
            previous_epoch,
            previous_terminal_sha256: canonical_record.sha256().to_owned(),
            previous_record_count,
            canonical_record,
            sha256: String::new(),
        };
        value.sha256 = value.digest()?;
        Ok(value)
    }

    pub(super) fn verify(
        &self,
        expected_epoch: u64,
        expected_previous_epoch: Option<u64>,
        expected_previous_count: usize,
        expected_canonical: &SlotMetadata,
    ) -> Result<(), SlotMetadataError> {
        let expected_previous_count = u64::try_from(expected_previous_count)
            .map_err(|_| corrupt("slot metadata epoch record count overflow"))?;
        if self.schema_version != EPOCH_SCHEMA_VERSION
            || self.epoch != expected_epoch
            || self.previous_epoch != expected_previous_epoch
            || &self.package != expected_canonical.package()
            || &self.slot != expected_canonical.slot()
            || self.previous_terminal_sha256 != expected_canonical.sha256()
            || self.previous_record_count != expected_previous_count
            || &self.canonical_record != expected_canonical
            || self.digest()? != self.sha256
        {
            return Err(corrupt("slot metadata epoch checkpoint mismatch"));
        }
        Ok(())
    }

    pub(super) const fn canonical_record(&self) -> &SlotMetadata {
        &self.canonical_record
    }

    pub(super) fn sha256(&self) -> &str {
        &self.sha256
    }

    fn digest(&self) -> Result<String, SlotMetadataError> {
        let mut unsigned = self.clone();
        unsigned.sha256.clear();
        digest_json(&unsigned)
            .map_err(|error| corrupt(&format!("slot metadata checkpoint digest failed: {error}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EpochCommit {
    schema_version: u32,
    epoch: u64,
    checkpoint_sha256: String,
    previous_commit_sha256: Option<String>,
    sha256: String,
}

impl EpochCommit {
    pub(super) fn new(
        epoch: u64,
        checkpoint_sha256: String,
        previous_commit_sha256: Option<String>,
    ) -> Result<Self, SlotMetadataError> {
        let mut value = Self {
            schema_version: EPOCH_SCHEMA_VERSION,
            epoch,
            checkpoint_sha256,
            previous_commit_sha256,
            sha256: String::new(),
        };
        value.sha256 = value.digest()?;
        Ok(value)
    }

    pub(super) fn verify(
        &self,
        expected_epoch: u64,
        expected_checkpoint_sha256: &str,
        expected_previous_commit_sha256: Option<&str>,
    ) -> Result<(), SlotMetadataError> {
        if self.schema_version != EPOCH_SCHEMA_VERSION
            || self.epoch != expected_epoch
            || self.checkpoint_sha256 != expected_checkpoint_sha256
            || self.previous_commit_sha256.as_deref() != expected_previous_commit_sha256
            || self.digest()? != self.sha256
        {
            return Err(corrupt("slot metadata epoch commit mismatch"));
        }
        Ok(())
    }

    pub(super) fn sha256(&self) -> &str {
        &self.sha256
    }

    fn digest(&self) -> Result<String, SlotMetadataError> {
        let mut unsigned = self.clone();
        unsigned.sha256.clear();
        digest_json(&unsigned)
            .map_err(|error| corrupt(&format!("slot metadata commit digest failed: {error}")))
    }
}

fn corrupt(message: &str) -> SlotMetadataError {
    SlotMetadataError::Corrupt(message.to_owned())
}
