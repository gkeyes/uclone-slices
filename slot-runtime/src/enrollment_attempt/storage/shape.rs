use std::fs;
use std::path::Path;

use super::{EnrollmentAttemptError, GENERATIONS, MARKER, reject_symlink_or_non_dir};

pub(super) fn validate_package_directory(package_dir: &Path) -> Result<(), EnrollmentAttemptError> {
    reject_symlink_or_non_dir(package_dir, "inspect enrollment attempt package")?;
    let entries = fs::read_dir(package_dir).map_err(|source| {
        EnrollmentAttemptError::io("read enrollment attempt package", package_dir, source)
    })?;
    let mut found_generations = false;
    for entry in entries {
        let entry = entry.map_err(|source| {
            EnrollmentAttemptError::io("read enrollment attempt artifact", package_dir, source)
        })?;
        let name = entry.file_name();
        let file_type = entry.file_type().map_err(|source| {
            EnrollmentAttemptError::io("inspect enrollment attempt artifact", &entry.path(), source)
        })?;
        if name == GENERATIONS && file_type.is_dir() && !found_generations {
            found_generations = true;
        } else if name != MARKER || !file_type.is_file() {
            return Err(EnrollmentAttemptError::Corrupt(format!(
                "unexpected enrollment attempt artifact in {}",
                package_dir.display()
            )));
        }
    }
    if found_generations {
        Ok(())
    } else {
        Err(EnrollmentAttemptError::Corrupt(
            "missing enrollment attempt generations".to_owned(),
        ))
    }
}

pub(super) fn valid_generation_name(name: &str) -> bool {
    name.strip_suffix(".json").is_some_and(|prefix| {
        prefix.len() == 16 && prefix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

pub(super) fn parse_generation(name: &str) -> Result<u64, EnrollmentAttemptError> {
    name.strip_suffix(".json")
        .ok_or_else(|| EnrollmentAttemptError::Corrupt("invalid generation filename".to_owned()))?
        .parse()
        .map_err(|_| EnrollmentAttemptError::Corrupt("invalid generation filename".to_owned()))
}
