#![cfg(all(unix, not(target_os = "android")))]
#![doc = "Host-only Android materializer filesystem operation tests."]
#![allow(
    dead_code,
    missing_docs,
    unreachable_pub,
    clippy::redundant_pub_crate,
    clippy::similar_names,
    reason = "production filesystem helpers are included selectively"
)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::{MetadataExt as _, symlink};
use std::path::{Path, PathBuf};

mod materializer {
    pub(crate) use uclone_slot_runtime::materializer::{ArtifactState, BackendFailure};
}

#[path = "../src/android/materializer/fsops.rs"]
mod fsops;

use materializer::{ArtifactState, BackendFailure};

#[derive(Debug, PartialEq, Eq)]
struct SentinelMetadata {
    device: u64,
    inode: u64,
    mode: u32,
    links: u64,
    uid: u32,
    gid: u32,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

fn paths(root: &Path) -> [PathBuf; 4] {
    [
        root.join("staging-ce"),
        root.join("staging-de"),
        root.join("ready-ce"),
        root.join("ready-de"),
    ]
}

fn code<T>(result: &Result<T, BackendFailure>) -> Option<&str> {
    result.as_ref().err().map(BackendFailure::code)
}

fn sentinel_metadata(path: &Path) -> Result<SentinelMetadata, Box<dyn Error>> {
    let metadata = fs::symlink_metadata(path)?;
    Ok(SentinelMetadata {
        device: metadata.dev(),
        inode: metadata.ino(),
        mode: metadata.mode(),
        links: metadata.nlink(),
        uid: metadata.uid(),
        gid: metadata.gid(),
        size: metadata.size(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
    })
}

#[test]
fn artifact_state_detects_partial_ce_and_de_shapes() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let [staging_ce, staging_de, ready_ce, ready_de] = paths(root.path());

    fs::create_dir(&staging_ce)?;
    assert_eq!(
        fsops::artifact_state_at(&staging_ce, &staging_de, &ready_ce, &ready_de)?,
        ArtifactState::StagingOnly
    );
    fs::remove_dir(&staging_ce)?;
    fs::create_dir(&ready_de)?;
    assert_eq!(
        fsops::artifact_state_at(&staging_ce, &staging_de, &ready_ce, &ready_de)?,
        ArtifactState::ReadyOnly
    );
    fs::create_dir(&staging_de)?;
    assert_eq!(
        fsops::artifact_state_at(&staging_ce, &staging_de, &ready_ce, &ready_de)?,
        ArtifactState::Both
    );
    Ok(())
}

#[test]
fn failed_second_publish_is_detectable_and_cleanup_preserves_base() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let ce = root.path().join("ce");
    let de = root.path().join("de");
    let base = root.path().join("base");
    fs::create_dir_all(&ce)?;
    fs::create_dir_all(&de)?;
    fs::create_dir_all(&base)?;

    let staging_ce = ce.join("staging");
    let ready_ce = ce.join("ready");
    let staging_de = de.join("staging");
    let ready_de = root.path().join("missing-de-parent/ready");
    fs::create_dir(&staging_ce)?;
    fs::create_dir(&staging_de)?;
    fs::write(staging_ce.join("payload"), b"ce")?;
    fs::write(staging_de.join("payload"), b"de")?;

    let sentinel = base.join("sentinel");
    fs::write(&sentinel, b"immutable-base")?;
    let before_bytes = fs::read(&sentinel)?;
    let before_metadata = sentinel_metadata(&sentinel)?;

    let publish = fsops::publish_at(&staging_ce, &staging_de, &ready_ce, &ready_de);
    assert_eq!(code(&publish), Some("publish_de_failed"));
    assert_eq!(
        fsops::artifact_state_at(&staging_ce, &staging_de, &ready_ce, &ready_de)?,
        ArtifactState::Both
    );

    fsops::cleanup_at([&staging_ce, &staging_de, &ready_ce, &ready_de])?;
    assert_eq!(
        fsops::artifact_state_at(&staging_ce, &staging_de, &ready_ce, &ready_de)?,
        ArtifactState::Absent
    );
    assert_eq!(fs::read(&sentinel)?, before_bytes);
    assert_eq!(sentinel_metadata(&sentinel)?, before_metadata);
    Ok(())
}

#[test]
fn symlink_and_non_directory_artifact_roots_are_rejected() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let [staging_ce, staging_de, ready_ce, ready_de] = paths(root.path());
    let target = root.path().join("target");
    fs::create_dir(&target)?;
    symlink(&target, &staging_ce)?;
    let symlink_state = fsops::artifact_state_at(&staging_ce, &staging_de, &ready_ce, &ready_de);
    assert_eq!(code(&symlink_state), Some("artifact_root_invalid"));

    fs::remove_file(&staging_ce)?;
    fs::write(&ready_de, b"not-a-directory")?;
    let file_state = fsops::artifact_state_at(&staging_ce, &staging_de, &ready_ce, &ready_de);
    assert_eq!(code(&file_state), Some("artifact_root_invalid"));
    Ok(())
}
