#[cfg(any(target_os = "android", target_os = "linux"))]
use std::fs::File;
#[cfg(any(target_os = "android", target_os = "linux"))]
use std::io::Read as _;
use std::io::{self, Write};
use std::path::Path;

use crate::daemon::{
    RuntimeOwnerMismatch, RuntimeOwnerVerdict, RuntimeOwnerVerificationError, verify_runtime_owner,
};
use crate::domain::BootId;
use crate::emergency_manifest::{
    EmergencyManifestError, EmergencyManifestStore, RuntimeOwnerProof,
};
use crate::layout::RuntimeLayout;

use super::{CliError, fail, next_request_id};

mod wire;

use wire::{EmergencySnapshot, EmergencyStatusFrame, OwnerVerdict};

/// Failure to prove a fixed, read-only emergency status snapshot.
#[derive(Debug, thiserror::Error)]
pub enum EmergencyStatusError {
    /// Reading or validating the fixed emergency manifest failed.
    #[error("manifest validation failed: {0}")]
    Manifest(#[from] EmergencyManifestError),
    /// The current Linux boot identifier could not be read.
    #[error("current boot probe failed: {0}")]
    BootProbe(#[source] io::Error),
    /// The current boot identifier was not a valid bounded domain value.
    #[error("current boot identifier is invalid")]
    InvalidBoot,
    /// The committed manifest belongs to another boot.
    #[error("manifest boot mismatch: expected {expected}, found {actual}")]
    StaleBoot {
        /// Current boot identifier.
        expected: String,
        /// Manifest boot identifier.
        actual: String,
    },
    /// The manifest owner proof names no current Runtime lock.
    #[error("runtime owner lock is missing")]
    OwnerMissing,
    /// The live Runtime identity did not match the manifest proof.
    #[error("runtime owner proof mismatch: {0:?}")]
    OwnerInvalid(RuntimeOwnerMismatch),
    /// The live owner probe could not establish a verdict.
    #[error("runtime owner verification failed: {0}")]
    OwnerProbe(#[from] RuntimeOwnerVerificationError),
    /// The requested page starts after the manifest package set.
    #[error("package cursor {cursor} exceeds total {total}")]
    CursorOutOfRange {
        /// Requested zero-based cursor.
        cursor: usize,
        /// Number of packages in the manifest.
        total: usize,
    },
    /// The independently versioned response could not be encoded.
    #[error("encode emergency status response")]
    Encode,
    /// The response exceeded the same bound as daemon protocol frames.
    #[error("emergency status response is too large: {0} bytes")]
    FrameTooLarge(usize),
}

pub(super) fn run<O: Write, E: Write>(
    cursor: u16,
    limit: u8,
    output: &mut O,
    diagnostics: &mut E,
) -> Result<(), CliError> {
    let request_id = next_request_id()?;
    let result = current_boot_id().and_then(|boot_id| {
        load_snapshot(
            &RuntimeLayout::emergency_manifest_root(),
            RuntimeLayout::lock(),
            &boot_id,
            usize::from(cursor),
            usize::from(limit),
            verify_runtime_owner,
        )
    });
    let frame = match result {
        Ok(snapshot) => EmergencyStatusFrame::success(request_id.as_str(), snapshot),
        Err(error) => {
            let frame = EmergencyStatusFrame::failure(request_id.as_str(), &error);
            write_frame(&frame, output)?;
            return fail(CliError::EmergencyStatus(error), diagnostics);
        }
    };
    write_frame(&frame, output)
}

fn load_snapshot<F>(
    root: &Path,
    lock: &Path,
    current_boot: &BootId,
    cursor: usize,
    limit: usize,
    verify_owner: F,
) -> Result<EmergencySnapshot, EmergencyStatusError>
where
    F: FnOnce(
        &Path,
        &RuntimeOwnerProof,
    ) -> Result<RuntimeOwnerVerdict, RuntimeOwnerVerificationError>,
{
    let store = EmergencyManifestStore::open_existing(root)?;
    let record = store.load_strict()?;
    let manifest = record.manifest();
    if manifest.boot_id() != current_boot {
        return Err(EmergencyStatusError::StaleBoot {
            expected: current_boot.as_str().to_owned(),
            actual: manifest.boot_id().as_str().to_owned(),
        });
    }
    let owner_verdict = match manifest.runtime_owner_proof() {
        None => OwnerVerdict::NotRequired,
        Some(proof) => match verify_owner(lock, proof)? {
            RuntimeOwnerVerdict::Valid => OwnerVerdict::Valid,
            RuntimeOwnerVerdict::Missing => return Err(EmergencyStatusError::OwnerMissing),
            RuntimeOwnerVerdict::Invalid(reason) => {
                return Err(EmergencyStatusError::OwnerInvalid(reason));
            }
        },
    };
    let total = manifest.packages().len();
    if cursor > total {
        return Err(EmergencyStatusError::CursorOutOfRange { cursor, total });
    }
    let end = cursor.saturating_add(limit).min(total);
    Ok(EmergencySnapshot::new(
        current_boot,
        &record,
        owner_verdict,
        cursor,
        end,
    ))
}

fn current_boot_id() -> Result<BootId, EmergencyStatusError> {
    #[cfg(any(target_os = "android", target_os = "linux"))]
    let raw = {
        let mut bytes = Vec::with_capacity(64);
        File::open("/proc/sys/kernel/random/boot_id")
            .map_err(EmergencyStatusError::BootProbe)?
            .take(65)
            .read_to_end(&mut bytes)
            .map_err(EmergencyStatusError::BootProbe)?;
        if bytes.len() > 64 {
            return Err(EmergencyStatusError::InvalidBoot);
        }
        String::from_utf8(bytes).map_err(|_| EmergencyStatusError::InvalidBoot)?
    };
    #[cfg(target_os = "macos")]
    let raw = "host-test-boot".to_owned();
    BootId::parse(raw.trim()).map_err(|_| EmergencyStatusError::InvalidBoot)
}

fn write_frame<O: Write>(frame: &EmergencyStatusFrame, output: &mut O) -> Result<(), CliError> {
    let mut bytes = serde_json::to_vec(frame).map_err(|_| EmergencyStatusError::Encode)?;
    bytes.push(b'\n');
    if bytes.len() > crate::protocol::MAX_FRAME_SIZE {
        return Err(EmergencyStatusError::FrameTooLarge(bytes.len()).into());
    }
    output.write_all(&bytes).map_err(CliError::Io)?;
    output.flush().map_err(CliError::Io)
}

#[cfg(test)]
mod tests;
