use std::fs;
use std::path::Path;

use super::{JournalError, JournalStep};
use crate::domain::TransactionId;
use crate::store_security::{self, StoreSecurityError};

pub(super) fn initialize_root(path: &Path) -> Result<u32, JournalError> {
    store_security::initialize_root(path, "journal")
        .map_err(|error| map_security("initialize journal root", path, error))
}

pub(super) fn ensure_directory(path: &Path, owner_uid: u32) -> Result<(), JournalError> {
    store_security::ensure_child_directory(path, owner_uid, "journal")
        .map_err(|error| map_security("create journal directory", path, error))
}

pub(super) fn validate_directory(path: &Path, owner_uid: u32) -> Result<(), JournalError> {
    store_security::validate_directory(path, owner_uid, "journal")
        .map_err(|error| map_security("validate journal directory", path, error))
}

pub(super) fn write_step(
    directory: &Path,
    owner_uid: u32,
    step: &JournalStep,
) -> Result<(), JournalError> {
    validate_directory(directory, owner_uid)?;
    let path = directory.join(step_file_name(step.generation()));
    let bytes = serde_json::to_vec(step)?;
    store_security::write_new_record(&path, &bytes, owner_uid, "journal step")
        .map_err(|error| map_security("publish journal step", &path, error))
}

pub(super) fn sync_directory(path: &Path, owner_uid: u32) -> Result<(), JournalError> {
    store_security::sync_directory(path, owner_uid, "journal")
        .map_err(|error| map_security("sync journal directory", path, error))
}

pub(super) fn read_steps(
    directory: &Path,
    owner_uid: u32,
    transaction_id: &TransactionId,
) -> Result<Vec<JournalStep>, JournalError> {
    validate_directory(directory, owner_uid)?;
    let entries = fs::read_dir(directory)
        .map_err(|source| JournalError::io("read transaction steps", directory, source))?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|source| JournalError::io("read transaction entry", directory, source))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            return Err(JournalError::Corrupt("non-UTF-8 step file".to_owned()));
        };
        let file_type = entry.file_type().map_err(|source| {
            JournalError::io("inspect transaction step", &entry.path(), source)
        })?;
        if name.starts_with('.') {
            if valid_temporary_step_name(name) && file_type.is_file() {
                store_security::validate_record(&entry.path(), owner_uid, "journal step").map_err(
                    |error| map_security("validate temporary journal step", &entry.path(), error),
                )?;
                continue;
            }
            return Err(JournalError::Corrupt(format!(
                "unexpected step artifact {name}"
            )));
        }
        if !valid_step_name(name) || !file_type.is_file() {
            return Err(JournalError::Corrupt(format!(
                "unexpected step artifact {name}"
            )));
        }
        files.push(entry.path());
    }
    files.sort();
    let mut steps = Vec::with_capacity(files.len());
    for path in files {
        let bytes = store_security::read_record(&path, owner_uid, "journal step")
            .map_err(|error| map_security("read journal step", &path, error))?;
        let step = serde_json::from_slice::<JournalStep>(&bytes).map_err(|source| {
            JournalError::Corrupt(format!("digest or JSON verification failed: {source}"))
        })?;
        step.verify()?;
        verify_chain(transaction_id, &steps, &step)?;
        steps.push(step);
    }
    Ok(steps)
}

pub(super) fn validate_transaction_directory(
    directory: &Path,
    owner_uid: u32,
) -> Result<(), JournalError> {
    validate_directory(directory, owner_uid)?;
    let entries = fs::read_dir(directory)
        .map_err(|source| JournalError::io("read transaction directory", directory, source))?;
    let mut found = false;
    for entry in entries {
        let entry = entry
            .map_err(|source| JournalError::io("read transaction entry", directory, source))?;
        let file_type = entry.file_type().map_err(|source| {
            JournalError::io("inspect transaction entry", &entry.path(), source)
        })?;
        if entry.file_name() != "steps" || !file_type.is_dir() || found {
            return Err(JournalError::Corrupt(format!(
                "unexpected transaction artifact in {}",
                directory.display()
            )));
        }
        validate_directory(&entry.path(), owner_uid)?;
        found = true;
    }
    if found {
        Ok(())
    } else {
        Err(JournalError::Corrupt(format!(
            "missing transaction steps in {}",
            directory.display()
        )))
    }
}

pub(super) fn step_file_name(generation: u64) -> String {
    format!("{generation:016}.json")
}

fn verify_chain(
    transaction_id: &TransactionId,
    existing: &[JournalStep],
    step: &JournalStep,
) -> Result<(), JournalError> {
    let expected_generation = u64::try_from(existing.len())
        .map_err(|_| JournalError::Corrupt("too many steps".to_owned()))?
        .checked_add(1)
        .ok_or_else(|| JournalError::Corrupt("generation overflow".to_owned()))?;
    if step.transaction_id() != transaction_id || step.generation() != expected_generation {
        return Err(JournalError::Corrupt(
            "transaction id or generation mismatch".to_owned(),
        ));
    }
    let expected_previous = existing.last().map(JournalStep::sha256);
    if step.previous_sha256() != expected_previous {
        return Err(JournalError::Corrupt("previous digest mismatch".to_owned()));
    }
    Ok(())
}

fn valid_step_name(name: &str) -> bool {
    let Some(prefix) = name.strip_suffix(".json") else {
        return false;
    };
    name.len() == 21 && prefix.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_temporary_step_name(name: &str) -> bool {
    let Some(name) = name.strip_prefix('.') else {
        return false;
    };
    let Some((step_name, suffix)) = name.split_once(".tmp-") else {
        return false;
    };
    valid_step_name(step_name) && valid_temporary_suffix(suffix)
}

fn valid_temporary_suffix(suffix: &str) -> bool {
    if suffix == "orphan" {
        return true;
    }
    suffix.split_once('-').is_some_and(|(process, sequence)| {
        !process.is_empty()
            && !sequence.is_empty()
            && process.bytes().all(|byte| byte.is_ascii_digit())
            && sequence.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn map_security(action: &'static str, path: &Path, error: StoreSecurityError) -> JournalError {
    match error {
        StoreSecurityError::Io(source) => JournalError::io(action, path, source),
        StoreSecurityError::Corrupt(message) => JournalError::Corrupt(message),
    }
}
