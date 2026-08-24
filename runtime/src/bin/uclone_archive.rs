use std::env;
use std::io::{Read, Write as _};
use std::path::{Component, Path, PathBuf};

use uclone_slices_runtime::archive::{
    ArchiveError, HelperRequest, HelperResponse, create_backup, inspect_backup, read_control_frame,
    restore_backup, write_response,
};

const MANAGER_PACKAGE: &str = "com.uclone.slices.v2";
const CE_SLOTS_ROOT: &str = "/data/misc_ce/0/uclone-slices-v2/slots";
const DE_SLOTS_ROOT: &str = "/data/misc_de/0/uclone-slices-v2/slots";
const CE_TRANSFERS_ROOT: &str = "/data/misc_ce/0/uclone-slices-v2/transfers";
const DE_TRANSFERS_ROOT: &str = "/data/misc_de/0/uclone-slices-v2/transfers";

fn main() {
    let response = match run() {
        Ok(response) => response,
        Err(error) => HelperResponse::Error {
            error: uclone_slices_runtime::archive::HelperErrorBody {
                code: error.wire_code().to_owned(),
            },
        },
    };
    let mut stdout = std::io::stdout().lock();
    if write_response(&mut stdout, &response).is_err() {
        std::process::exit(2);
    }
    if matches!(response, HelperResponse::Error { .. }) {
        std::process::exit(1);
    }
}

fn run() -> Result<HelperResponse, ArchiveError> {
    reject_arguments()?;
    let mut stdin = std::io::stdin().lock();
    let request = read_control_frame(&mut stdin)?;
    require_end_of_input(&mut stdin)?;
    match request {
        HelperRequest::Backup(request) => {
            validate_manager_cache_path(&request.output_path, false)?;
            for source in &request.sources {
                validate_backup_source(
                    &request.package,
                    &source.source_slot,
                    &source.ce_path,
                    &source.de_path,
                )?;
            }
            create_backup(request).map(|manifest| HelperResponse::Manifest { manifest })
        }
        HelperRequest::Inspect {
            input_path,
            password,
        } => {
            validate_manager_cache_path(&input_path, true)?;
            inspect_backup(&input_path, password)
                .map(|manifest| HelperResponse::Manifest { manifest })
        }
        HelperRequest::Restore(request) => {
            validate_manager_cache_path(&request.input_path, true)?;
            for destination in &request.destinations {
                validate_transfer_destination(&destination.ce_path, true)?;
                validate_transfer_destination(&destination.de_path, false)?;
            }
            restore_backup(request).map(|manifest| HelperResponse::Manifest { manifest })
        }
    }
}

fn reject_arguments() -> Result<(), ArchiveError> {
    (env::args_os().count() == 1)
        .then_some(())
        .ok_or(ArchiveError::Invalid)
}

fn require_end_of_input(reader: &mut impl Read) -> Result<(), ArchiveError> {
    let mut trailing = [0_u8; 1];
    match reader.read(&mut trailing) {
        Ok(0) => Ok(()),
        Ok(_) | Err(_) => Err(ArchiveError::Invalid),
    }
}

fn validate_manager_cache_path(path: &Path, must_exist: bool) -> Result<(), ArchiveError> {
    validate_absolute_normal_path(path)?;
    let allowed = [
        PathBuf::from("/data/user/0")
            .join(MANAGER_PACKAGE)
            .join("cache"),
        PathBuf::from("/data/data")
            .join(MANAGER_PACKAGE)
            .join("cache"),
    ];
    let root = allowed
        .iter()
        .find(|root| path.starts_with(root))
        .ok_or(ArchiveError::Invalid)?;
    if path.file_name().is_none() || path == root {
        return Err(ArchiveError::Invalid);
    }
    require_real_parent_chain(root, path)?;
    let metadata = std::fs::symlink_metadata(path);
    match (must_exist, metadata) {
        (true, Ok(metadata)) if metadata.file_type().is_file() => Ok(()),
        (false, Err(error)) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(ArchiveError::Invalid),
    }
}

fn require_real_parent_chain(root: &Path, path: &Path) -> Result<(), ArchiveError> {
    let parent = path.parent().ok_or(ArchiveError::Invalid)?;
    let relative = parent
        .strip_prefix(root)
        .map_err(|_error| ArchiveError::Invalid)?;
    require_real_directory(root)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(ArchiveError::Invalid);
        };
        current.push(value);
        require_real_directory(&current)?;
    }
    Ok(())
}

fn validate_backup_source(
    package: &str,
    slot: &str,
    ce_path: &Path,
    de_path: &Path,
) -> Result<(), ArchiveError> {
    validate_package(package)?;
    validate_identifier(slot)?;
    let expected = if slot == "base" {
        (
            PathBuf::from("/data/user/0").join(package),
            PathBuf::from("/data/user_de/0").join(package),
        )
    } else {
        (
            PathBuf::from(CE_SLOTS_ROOT).join(package).join(slot),
            PathBuf::from(DE_SLOTS_ROOT).join(package).join(slot),
        )
    };
    if ce_path != expected.0 || de_path != expected.1 {
        return Err(ArchiveError::Invalid);
    }
    require_real_directory(ce_path)?;
    require_real_directory(de_path)
}

fn validate_transfer_destination(
    path: &Path,
    credential_encrypted: bool,
) -> Result<(), ArchiveError> {
    validate_absolute_normal_path(path)?;
    let root = Path::new(if credential_encrypted {
        CE_TRANSFERS_ROOT
    } else {
        DE_TRANSFERS_ROOT
    });
    let relative = path
        .strip_prefix(root)
        .map_err(|_error| ArchiveError::Invalid)?;
    let components = relative.components().collect::<Vec<_>>();
    if components.len() != 3 {
        return Err(ArchiveError::Invalid);
    }
    let values = components
        .iter()
        .map(|component| match component {
            Component::Normal(value) => value.to_str().ok_or(ArchiveError::Invalid),
            _ => Err(ArchiveError::Invalid),
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_identifier(values[0])?;
    if values[0].len() != 32 {
        return Err(ArchiveError::Invalid);
    }
    validate_package(values[1])?;
    validate_identifier(values[2])?;
    require_real_directory(path)
}

fn validate_absolute_normal_path(path: &Path) -> Result<(), ArchiveError> {
    if !path.is_absolute()
        || path.components().any(|component| {
            !matches!(component, Component::RootDir | Component::Normal(_))
                || component.as_os_str().to_str().is_none()
        })
    {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn validate_package(value: &str) -> Result<(), ArchiveError> {
    if value.len() > 255
        || value.split('.').count() < 2
        || value.split('.').any(|part| {
            part.is_empty()
                || part.len() > 63
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
    {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), ArchiveError> {
    if value.is_empty()
        || value.len() > 64
        || value == "."
        || value == ".."
        || value.starts_with('.')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

fn require_real_directory(path: &Path) -> Result<(), ArchiveError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        let _ = writeln!(
            std::io::stderr(),
            "archive helper path check failed: {error}"
        );
        ArchiveError::Invalid
    })?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(ArchiveError::Invalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn manager_cache_parent_chain_rejects_an_intermediate_symlink() {
        let root = tempfile::tempdir().unwrap();
        let cache = root.path().join("cache");
        let outside = root.path().join("outside");
        std::fs::create_dir(&cache).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::create_dir(cache.join("real")).unwrap();
        symlink(&outside, cache.join("escape")).unwrap();

        assert!(require_real_parent_chain(&cache, &cache.join("real/archive")).is_ok());
        assert!(require_real_parent_chain(&cache, &cache.join("escape/archive")).is_err());
    }
}
