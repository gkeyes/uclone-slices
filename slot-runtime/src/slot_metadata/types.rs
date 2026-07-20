use serde::{Deserialize, Serialize};

use super::SlotMetadataError;

#[doc = "Validated user-visible slot label that never participates in path derivation."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SlotDisplayName(String);

impl SlotDisplayName {
    #[doc = "Parses 1 to 48 visible Unicode characters."]
    pub fn parse(value: &str) -> Result<Self, SlotMetadataError> {
        let length = value.chars().count();
        if !(1..=48).contains(&length)
            || value.trim() != value
            || value.chars().any(char::is_control)
        {
            return Err(SlotMetadataError::Invalid(
                "display name must contain 1 to 48 visible characters".to_owned(),
            ));
        }
        Ok(Self(value.to_owned()))
    }

    #[doc = "Returns the display-only label."]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SlotDisplayName {
    type Error = SlotMetadataError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<SlotDisplayName> for String {
    fn from(value: SlotDisplayName) -> Self {
        value.0
    }
}

#[doc = "How a non-base slot received its initial CE and DE contents."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotSeedMode {
    #[doc = "Create empty CE and DE roots with the enrolled security profile."]
    Blank,
    #[doc = "Copy the immutable Android base view once."]
    CloneBase,
}

#[doc = "Durable lifecycle of one immutable slot identifier."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotRecordState {
    #[doc = "Paired storage is being materialized."]
    Creating,
    #[doc = "Paired storage and catalog proof are ready."]
    Ready,
    #[doc = "The slot is retained but cannot be activated."]
    Quarantined,
    #[doc = "The slot was retired and cannot be reused."]
    Deleted,
}
