use serde::{Deserialize, Serialize};

use super::model::SCHEMA_VERSION;
use super::{RescueError, RescueEvent, RescueId};
use crate::integrity::digest_json;

#[doc = "One verified generation in the rescue hash chain."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RescueStep {
    schema_version: u32,
    rescue_id: RescueId,
    generation: u64,
    previous_sha256: Option<String>,
    event: RescueEvent,
    sha256: String,
}

impl RescueStep {
    #[doc = "Returns the one-based generation."]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[doc = "Returns the previous generation digest when present."]
    pub fn previous_sha256(&self) -> Option<&str> {
        self.previous_sha256.as_deref()
    }

    #[doc = "Returns this generation's lowercase SHA-256 digest."]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    #[doc = "Returns the persisted rescue event."]
    pub const fn event(&self) -> &RescueEvent {
        &self.event
    }

    pub(super) fn new(
        rescue_id: RescueId,
        generation: u64,
        previous_sha256: Option<String>,
        event: RescueEvent,
    ) -> Result<Self, RescueError> {
        event.validate_standalone()?;
        let unsigned =
            UnsignedStep::new(&rescue_id, generation, previous_sha256.as_deref(), &event);
        let sha256 = digest_json(&unsigned)?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            rescue_id,
            generation,
            previous_sha256,
            event,
            sha256,
        })
    }

    pub(super) fn verify(&self) -> Result<(), RescueError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(RescueError::Corrupt(
                "unsupported rescue step schema".to_owned(),
            ));
        }
        self.event.validate_standalone()?;
        let unsigned = UnsignedStep::new(
            &self.rescue_id,
            self.generation,
            self.previous_sha256.as_deref(),
            &self.event,
        );
        if digest_json(&unsigned)? != self.sha256 {
            return Err(RescueError::Corrupt(
                "rescue step digest mismatch".to_owned(),
            ));
        }
        Ok(())
    }

    pub(super) const fn rescue_id(&self) -> &RescueId {
        &self.rescue_id
    }
}

#[derive(Serialize)]
struct UnsignedStep<'a> {
    schema_version: u32,
    rescue_id: &'a RescueId,
    generation: u64,
    previous_sha256: Option<&'a str>,
    event: &'a RescueEvent,
}

impl<'a> UnsignedStep<'a> {
    const fn new(
        rescue_id: &'a RescueId,
        generation: u64,
        previous_sha256: Option<&'a str>,
        event: &'a RescueEvent,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            rescue_id,
            generation,
            previous_sha256,
            event,
        }
    }
}
