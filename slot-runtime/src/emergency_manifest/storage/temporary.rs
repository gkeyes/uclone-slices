use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::super::EmergencyManifestError;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn path(record: &Path) -> Result<PathBuf, EmergencyManifestError> {
    let name = record
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            EmergencyManifestError::Corrupt("invalid emergency manifest file name".to_owned())
        })?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    Ok(record.with_file_name(format!(".{name}.tmp-{}-{sequence}", std::process::id())))
}
