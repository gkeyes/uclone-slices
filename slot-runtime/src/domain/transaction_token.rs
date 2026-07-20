use std::fmt;

use serde::{Deserialize, Serialize};

use super::DomainError;

#[doc = "Validated idempotency and journal transaction identifier."]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TransactionId(String);

impl TransactionId {
    #[doc = "Parses a stable transaction identifier safe for directory names."]
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        if valid_identifier(raw, 8, 64) {
            Ok(Self(raw.to_owned()))
        } else {
            Err(DomainError::InvalidTransactionId(raw.to_owned()))
        }
    }

    #[doc = "Returns the validated identifier."]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TransactionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for TransactionId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<TransactionId> for String {
    fn from(value: TransactionId) -> Self {
        value.0
    }
}

#[doc = "Validated transaction commit nonce."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct CommitNonce(String);

impl CommitNonce {
    #[doc = "Parses a nonce safe for durable records and equality checks."]
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        if valid_token(raw, 8, 64) {
            Ok(Self(raw.to_owned()))
        } else {
            Err(DomainError::InvalidCommitNonce)
        }
    }

    #[doc = "Returns the validated nonce."]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CommitNonce {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<CommitNonce> for String {
    fn from(value: CommitNonce) -> Self {
        value.0
    }
}

#[doc = "Validated Linux boot identifier captured before mutation."]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BootId(String);

impl BootId {
    #[doc = "Parses a boot identifier safe for durable records."]
    pub fn parse(raw: &str) -> Result<Self, DomainError> {
        if valid_token(raw, 8, 64) {
            Ok(Self(raw.to_owned()))
        } else {
            Err(DomainError::InvalidBootId)
        }
    }

    #[doc = "Returns the validated boot identifier."]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for BootId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<BootId> for String {
    fn from(value: BootId) -> Self {
        value.0
    }
}

fn valid_token(value: &str, minimum: usize, maximum: usize) -> bool {
    (minimum..=maximum).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
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
