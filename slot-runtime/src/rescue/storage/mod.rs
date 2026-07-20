use std::fs;
use std::path::{Path, PathBuf};

use super::{RescueError, RescueId, RescueStep};
use crate::domain::PackageName;

mod file;
mod shape;

pub(super) const MAX_RECORD_BYTES: usize = 16 * 1024;
pub(super) const MAX_RECORD_BYTES_U64: u64 = 16 * 1024;
pub(super) const MAX_STEPS: usize = 32;
pub(super) const PACKAGES: &str = "packages";
pub(super) const RESCUE: &str = "rescue";
pub(super) const STEPS: &str = "steps";

#[derive(Debug, Clone)]
pub(super) struct StorePaths {
    pub(super) root: PathBuf,
    pub(super) packages: PathBuf,
    pub(super) package: PathBuf,
    pub(super) rescue: PathBuf,
    pub(super) steps: PathBuf,
    pub(super) owner_uid: u32,
}

pub(super) fn initialize(
    root: &Path,
    package_name: &PackageName,
) -> Result<StorePaths, RescueError> {
    let owner_uid = shape::ensure_root(root)?;
    let packages = root.join(PACKAGES);
    shape::ensure_child_directory(&packages, owner_uid)?;
    let package = packages.join(package_name.as_str());
    let rescue = package.join(RESCUE);
    let steps = rescue.join(STEPS);
    let paths = StorePaths {
        root: root.to_path_buf(),
        packages,
        package,
        rescue,
        steps,
        owner_uid,
    };
    shape::validate_store(&paths)?;
    Ok(paths)
}

pub(super) fn publish_first(paths: &StorePaths, step: &RescueStep) -> Result<(), RescueError> {
    shape::validate_store(paths)?;
    if paths.package.exists() {
        return Err(RescueError::Corrupt(
            "rescue package epoch already exists".to_owned(),
        ));
    }
    shape::ensure_child_directory(&paths.package, paths.owner_uid)?;
    let staging = paths.package.join(".rescue-new");
    if staging.exists() {
        return Err(RescueError::Corrupt(
            "rescue staging artifact already exists".to_owned(),
        ));
    }
    shape::ensure_child_directory(&staging, paths.owner_uid)?;
    let staging_steps = staging.join(STEPS);
    shape::ensure_child_directory(&staging_steps, paths.owner_uid)?;
    write_step_to(&staging_steps, paths.owner_uid, step)?;
    shape::sync_directory(&staging, paths.owner_uid)?;
    fs::rename(&staging, &paths.rescue)
        .map_err(|source| RescueError::io("publish rescue epoch", &paths.rescue, source))?;
    shape::sync_directory(&paths.package, paths.owner_uid)?;
    shape::validate_store(paths)
}

pub(super) fn write_step(paths: &StorePaths, step: &RescueStep) -> Result<(), RescueError> {
    shape::validate_store(paths)?;
    write_step_to(&paths.steps, paths.owner_uid, step)
}

pub(super) fn read_steps(
    paths: &StorePaths,
    rescue_id: Option<&RescueId>,
) -> Result<Vec<RescueStep>, RescueError> {
    shape::validate_store(paths)?;
    if !paths.rescue.exists() {
        return Ok(Vec::new());
    }
    let entries = shape::bounded_entries(&paths.steps, MAX_STEPS)?;
    let mut files = Vec::with_capacity(entries.len());
    for entry in entries {
        let name = entry.file_name();
        let name = name
            .to_str()
            .ok_or_else(|| RescueError::Corrupt("non-UTF-8 rescue step".to_owned()))?;
        if !valid_step_name(name) {
            return Err(RescueError::Corrupt(format!(
                "unexpected rescue step artifact {name}"
            )));
        }
        files.push(entry.path());
    }
    files.sort();
    decode_steps(paths.owner_uid, rescue_id, files)
}

pub(super) fn step_file_name(generation: u64) -> String {
    format!("{generation:016}.json")
}

fn write_step_to(directory: &Path, owner_uid: u32, step: &RescueStep) -> Result<(), RescueError> {
    let bytes = serde_json::to_vec(step)?;
    let path = directory.join(step_file_name(step.generation()));
    file::write_new(&path, &bytes, owner_uid)
}

fn decode_steps(
    owner_uid: u32,
    rescue_id: Option<&RescueId>,
    files: Vec<PathBuf>,
) -> Result<Vec<RescueStep>, RescueError> {
    let mut loaded = Vec::with_capacity(files.len());
    for path in files {
        let bytes = file::read_bounded(&path, owner_uid)?;
        let step = serde_json::from_slice::<RescueStep>(&bytes)
            .map_err(|source| RescueError::Corrupt(format!("invalid rescue JSON: {source}")))?;
        step.verify()?;
        verify_chain(rescue_id, &loaded, &step)?;
        loaded.push(step);
    }
    if loaded.is_empty() {
        return Err(RescueError::Corrupt(
            "published rescue has no steps".to_owned(),
        ));
    }
    Ok(loaded)
}

fn verify_chain(
    rescue_id: Option<&RescueId>,
    loaded: &[RescueStep],
    step: &RescueStep,
) -> Result<(), RescueError> {
    let expected = u64::try_from(loaded.len())
        .map_err(|_| RescueError::BoundExceeded("step generation"))?
        .checked_add(1)
        .ok_or(RescueError::BoundExceeded("step generation"))?;
    if step.generation() != expected || rescue_id.is_some_and(|id| step.rescue_id() != id) {
        return Err(RescueError::Corrupt(
            "rescue id or generation mismatch".to_owned(),
        ));
    }
    if step.previous_sha256() != loaded.last().map(RescueStep::sha256) {
        return Err(RescueError::Corrupt(
            "rescue previous digest mismatch".to_owned(),
        ));
    }
    Ok(())
}

fn valid_step_name(name: &str) -> bool {
    name.strip_suffix(".json")
        .is_some_and(|prefix| name.len() == 21 && prefix.bytes().all(|byte| byte.is_ascii_digit()))
}
