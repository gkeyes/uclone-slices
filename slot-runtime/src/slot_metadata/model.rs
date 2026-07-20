use serde::{Deserialize, Serialize};

use crate::domain::{PackageName, SlotId};
use crate::integrity::digest_json;

use super::{SlotDisplayName, SlotMetadataError, SlotRecordState, SlotSeedMode};

pub(super) const SCHEMA_VERSION: u32 = 1;

#[doc = "One verified revision in a package-local slot metadata stream."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotMetadata {
    schema_version: u32,
    generation: u64,
    package: PackageName,
    slot: SlotId,
    display_name: SlotDisplayName,
    seed_mode: SlotSeedMode,
    state: SlotRecordState,
    created_version_code: u64,
    last_opened_version_code: u64,
    previous_sha256: Option<String>,
    sha256: String,
}

impl SlotMetadata {
    pub(super) fn initial(
        package: PackageName,
        slot: SlotId,
        display_name: SlotDisplayName,
        seed_mode: SlotSeedMode,
        version_code: u64,
    ) -> Result<Self, SlotMetadataError> {
        if slot.is_base() || version_code == 0 {
            return Err(SlotMetadataError::Invalid(
                "slot metadata requires a non-base slot and non-zero version".to_owned(),
            ));
        }
        Self::build(
            1,
            package,
            slot,
            display_name,
            seed_mode,
            SlotRecordState::Creating,
            version_code,
            version_code,
            None,
        )
    }

    pub(super) fn next(
        &self,
        display_name: SlotDisplayName,
        state: SlotRecordState,
        opened_version: u64,
    ) -> Result<Self, SlotMetadataError> {
        validate_transition(self.state, state)?;
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| SlotMetadataError::Corrupt("slot generation overflow".to_owned()))?;
        Self::build(
            generation,
            self.package.clone(),
            self.slot.clone(),
            display_name,
            self.seed_mode,
            state,
            self.created_version_code,
            opened_version,
            Some(self.sha256.clone()),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        generation: u64,
        package: PackageName,
        slot: SlotId,
        display_name: SlotDisplayName,
        seed_mode: SlotSeedMode,
        state: SlotRecordState,
        created_version_code: u64,
        last_opened_version_code: u64,
        previous_sha256: Option<String>,
    ) -> Result<Self, SlotMetadataError> {
        let mut value = Self {
            schema_version: SCHEMA_VERSION,
            generation,
            package,
            slot,
            display_name,
            seed_mode,
            state,
            created_version_code,
            last_opened_version_code,
            previous_sha256,
            sha256: String::new(),
        };
        value.sha256 = value.digest()?;
        Ok(value)
    }

    pub(super) fn verify(&self, previous: Option<&Self>) -> Result<(), SlotMetadataError> {
        let expected_generation = match previous {
            Some(value) => value
                .generation
                .checked_add(1)
                .ok_or_else(|| SlotMetadataError::Corrupt("slot generation overflow".to_owned()))?,
            None => 1,
        };
        if self.schema_version != SCHEMA_VERSION
            || self.generation != expected_generation
            || self.slot.is_base()
            || self.created_version_code == 0
            || self.last_opened_version_code == 0
            || self.previous_sha256.as_deref() != previous.map(Self::sha256)
            || self.digest()? != self.sha256
        {
            return Err(SlotMetadataError::Corrupt(
                "slot metadata integrity check failed".to_owned(),
            ));
        }
        if let Some(previous) = previous {
            if self.package != previous.package
                || self.slot != previous.slot
                || self.seed_mode != previous.seed_mode
                || self.created_version_code != previous.created_version_code
            {
                return Err(SlotMetadataError::Corrupt(
                    "immutable slot metadata changed".to_owned(),
                ));
            }
            validate_transition(previous.state, self.state)?;
        }
        Ok(())
    }

    fn digest(&self) -> Result<String, SlotMetadataError> {
        let mut unsigned = self.clone();
        unsigned.sha256.clear();
        digest_json(&unsigned).map_err(|error| {
            SlotMetadataError::Corrupt(format!("slot metadata digest failed: {error}"))
        })
    }

    #[doc = "Returns the owning package."]
    pub const fn package(&self) -> &PackageName {
        &self.package
    }
    #[doc = "Returns the immutable internal identifier."]
    pub const fn slot(&self) -> &SlotId {
        &self.slot
    }
    #[doc = "Returns the display-only label."]
    pub const fn display_name(&self) -> &SlotDisplayName {
        &self.display_name
    }
    #[doc = "Returns the initial seed mode."]
    pub const fn seed_mode(&self) -> SlotSeedMode {
        self.seed_mode
    }
    #[doc = "Returns the durable lifecycle state."]
    pub const fn state(&self) -> SlotRecordState {
        self.state
    }
    #[doc = "Returns the creation version code."]
    pub const fn created_version_code(&self) -> u64 {
        self.created_version_code
    }
    #[doc = "Returns the latest opened version code."]
    pub const fn last_opened_version_code(&self) -> u64 {
        self.last_opened_version_code
    }
    #[doc = "Returns the stream generation."]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    #[doc = "Returns this revision digest."]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
}

fn validate_transition(
    previous: SlotRecordState,
    next: SlotRecordState,
) -> Result<(), SlotMetadataError> {
    let valid = match previous {
        SlotRecordState::Creating => matches!(
            next,
            SlotRecordState::Ready | SlotRecordState::Quarantined | SlotRecordState::Deleted
        ),
        SlotRecordState::Ready => matches!(
            next,
            SlotRecordState::Ready | SlotRecordState::Quarantined | SlotRecordState::Deleted
        ),
        SlotRecordState::Quarantined => matches!(
            next,
            SlotRecordState::Quarantined | SlotRecordState::Deleted
        ),
        SlotRecordState::Deleted => false,
    };
    valid.then_some(()).ok_or_else(|| {
        SlotMetadataError::Invalid(format!(
            "illegal slot transition from {previous:?} to {next:?}"
        ))
    })
}
