use serde::{Deserialize, Serialize};

use super::{JournalError, JournalEvent};
use crate::domain::TransactionId;
use crate::integrity::digest_json;

const SCHEMA_VERSION: u32 = 2;

#[doc = "One verified hash-chained journal record."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalStep {
    schema_version: u32,
    transaction_id: TransactionId,
    generation: u64,
    previous_sha256: Option<String>,
    event: JournalEvent,
    sha256: String,
}

impl JournalStep {
    #[doc = "Returns the one-based step generation."]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[doc = "Returns the previous step digest when this is not the first step."]
    pub fn previous_sha256(&self) -> Option<&str> {
        self.previous_sha256.as_deref()
    }

    #[doc = "Returns this step's lowercase SHA-256 digest."]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[doc = "Returns the persisted event."]
    pub const fn event(&self) -> &JournalEvent {
        &self.event
    }

    pub(super) fn new(
        transaction_id: TransactionId,
        generation: u64,
        previous_sha256: Option<String>,
        event: JournalEvent,
    ) -> Result<Self, JournalError> {
        let unsigned = UnsignedStep::new(
            &transaction_id,
            generation,
            previous_sha256.as_deref(),
            &event,
        );
        let sha256 = digest_json(&unsigned)?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            transaction_id,
            generation,
            previous_sha256,
            event,
            sha256,
        })
    }

    pub(super) fn verify(&self) -> Result<(), JournalError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(JournalError::Corrupt(format!(
                "unsupported schema version {}",
                self.schema_version
            )));
        }
        let unsigned = UnsignedStep::new(
            &self.transaction_id,
            self.generation,
            self.previous_sha256.as_deref(),
            &self.event,
        );
        if digest_json(&unsigned)? != self.sha256 {
            return Err(JournalError::Corrupt("digest mismatch".to_owned()));
        }
        Ok(())
    }

    pub(super) const fn transaction_id(&self) -> &TransactionId {
        &self.transaction_id
    }
}

#[derive(Serialize)]
struct UnsignedStep<'a> {
    schema_version: u32,
    transaction_id: &'a TransactionId,
    generation: u64,
    previous_sha256: Option<&'a str>,
    event: &'a JournalEvent,
}

impl<'a> UnsignedStep<'a> {
    const fn new(
        transaction_id: &'a TransactionId,
        generation: u64,
        previous_sha256: Option<&'a str>,
        event: &'a JournalEvent,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            transaction_id,
            generation,
            previous_sha256,
            event,
        }
    }
}
