use std::fs::File;
use std::io::{Read as _, Result as IoResult};
use std::path::Path;

use crate::domain::{BootId, CommitNonce, TransactionId};
use crate::rescue::{RescueError, RescueId, RescueMetadata, RescueMetadataSource};
use crate::runtime::SwitchMetadata;
use crate::service::ServiceError;

const BOOT_ID_PATH: &str = "/proc/sys/kernel/random/boot_id";
const RANDOM_UUID_PATH: &str = "/proc/sys/kernel/random/uuid";
const MAX_TOKEN_BYTES: u64 = 65;
const MAX_TOKEN_BYTES_USIZE: usize = 65;

/// Supplies bounded transaction identifiers without accepting caller input.
pub trait MetadataSource: core::fmt::Debug {
    /// Returns fresh validated switch metadata for the current Linux boot.
    fn next(&mut self) -> Result<SwitchMetadata, ServiceError>;
}

/// Fixed `/proc` metadata source used by the production Android daemon.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemMetadataSource;

impl SystemMetadataSource {
    /// Creates the zero-sized fixed-path metadata source.
    pub const fn new() -> Self {
        Self
    }
}

impl MetadataSource for SystemMetadataSource {
    fn next(&mut self) -> Result<SwitchMetadata, ServiceError> {
        let uuid = read_token(Path::new(RANDOM_UUID_PATH))?;
        let boot = read_token(Path::new(BOOT_ID_PATH))?;
        let transaction =
            TransactionId::parse(&format!("tx-{uuid}")).map_err(|_| ServiceError::Internal)?;
        let nonce =
            CommitNonce::parse(&format!("nonce-{uuid}")).map_err(|_| ServiceError::Internal)?;
        let boot_id = BootId::parse(&boot).map_err(|_| ServiceError::Internal)?;
        Ok(SwitchMetadata::new(transaction, nonce, boot_id))
    }
}

impl RescueMetadataSource for SystemMetadataSource {
    fn next_rescue(&mut self) -> Result<RescueMetadata, RescueError> {
        let uuid = read_token(Path::new(RANDOM_UUID_PATH))
            .map_err(|_| RescueError::InvalidSpec("rescue uuid unavailable".to_owned()))?;
        let boot = read_token(Path::new(BOOT_ID_PATH))
            .map_err(|_| RescueError::InvalidSpec("boot id unavailable".to_owned()))?;
        let rescue_id = RescueId::parse(&format!("rescue-{uuid}"))?;
        let nonce = CommitNonce::parse(&format!("nonce-{uuid}"))?;
        let boot_id = BootId::parse(&boot)?;
        Ok(RescueMetadata::new(rescue_id, nonce, boot_id))
    }
}

fn read_token(path: &Path) -> Result<String, ServiceError> {
    let bytes = read_bounded(path).map_err(|_| ServiceError::Internal)?;
    let raw = core::str::from_utf8(&bytes).map_err(|_| ServiceError::Internal)?;
    let token = raw.trim_end_matches(['\n', '\r']);
    if token.is_empty()
        || token.len() > 64
        || token
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || byte == b'-'))
    {
        return Err(ServiceError::Internal);
    }
    Ok(token.to_owned())
}

fn read_bounded(path: &Path) -> IoResult<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_TOKEN_BYTES)
        .read_to_end(&mut bytes)?;
    if bytes.len() >= MAX_TOKEN_BYTES_USIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "fixed token exceeds bound",
        ));
    }
    Ok(bytes)
}
