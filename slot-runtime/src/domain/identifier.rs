use std::fmt;

use serde::{Deserialize, Serialize};

use super::DomainError;

#[doc = "Validated Android package name."]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PackageName(String);

impl PackageName {
    #[doc = "Parses an Android package name with at least two Java-style segments."]
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        if raw.len() > 255 {
            return Err(DomainError::InvalidPackageName(raw.to_owned()));
        }
        let mut segments = raw.split('.');
        let Some(first) = segments.next() else {
            return Err(DomainError::InvalidPackageName(raw.to_owned()));
        };
        if !valid_package_segment(first) || !segments.clone().all(valid_package_segment) {
            return Err(DomainError::InvalidPackageName(raw.to_owned()));
        }
        if segments.next().is_none() {
            return Err(DomainError::InvalidPackageName(raw.to_owned()));
        }
        Ok(Self(raw.to_owned()))
    }

    #[doc = "Returns the validated package name."]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PackageName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for PackageName {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<PackageName> for String {
    fn from(value: PackageName) -> Self {
        value.0
    }
}

#[doc = "Validated internal slot identifier."]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SlotId(String);

impl SlotId {
    #[doc = "Returns the reserved Android-owned base slot identifier."]
    pub fn base() -> Self {
        Self(crate::target::BASE_SLOT.to_owned())
    }

    #[doc = "Parses a lower-case identifier safe for Registry-derived paths."]
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        if valid_identifier(raw, 1, 64) {
            Ok(Self(raw.to_owned()))
        } else {
            Err(DomainError::InvalidSlotId(raw.to_owned()))
        }
    }

    #[doc = "Returns true only for the Android-owned base slot."]
    pub fn is_base(&self) -> bool {
        self.0 == crate::target::BASE_SLOT
    }

    #[doc = "Returns the validated identifier."]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SlotId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for SlotId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<SlotId> for String {
    fn from(value: SlotId) -> Self {
        value.0
    }
}

fn valid_package_segment(segment: &str) -> bool {
    let mut bytes = segment.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    first.is_ascii_alphabetic() && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn valid_identifier(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.len())
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}
