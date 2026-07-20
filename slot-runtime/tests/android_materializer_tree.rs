#![cfg(all(unix, not(target_os = "android")))]
#![allow(
    missing_docs,
    unreachable_pub,
    clippy::redundant_pub_crate,
    reason = "included production tree modules retain their crate-relative visibility"
)]

use std::error::Error;
use std::fs;
use std::os::unix::fs::{MetadataExt as _, symlink};
use std::os::unix::net::UnixListener;

mod materializer {
    pub use uclone_slot_runtime::materializer::TreeSafetyProof;
}

#[path = "../src/android/materializer/limits.rs"]
mod limits;
#[path = "../src/android/materializer/tree.rs"]
mod tree;

use limits::MaterializerLimits;
use materializer::TreeSafetyProof;
use tree::{TreeError, inspect_tree, sync_tree};

fn limits(
    depth: usize,
    entries: u64,
    file: u64,
    total: u64,
) -> Result<MaterializerLimits, Box<dyn Error>> {
    MaterializerLimits::bounded(depth, entries, file, total)
        .ok_or_else(|| "invalid test limits".into())
}

#[test]
fn digest_is_deterministic_and_content_sensitive() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("nested"))?;
    fs::write(root.path().join("zeta"), b"last")?;
    fs::write(root.path().join("nested/alpha"), b"first")?;
    let first = inspect_tree(root.path(), MaterializerLimits::PRODUCTION)?;
    let second = inspect_tree(root.path(), MaterializerLimits::PRODUCTION)?;
    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.safety(), TreeSafetyProof::clean());
    assert_ne!(first.device_id(), 0);
    assert_ne!(first.inode(), 0);
    assert_eq!(
        first.mode(),
        fs::symlink_metadata(root.path())?.mode() & 0o7777
    );
    let _identity = (first.uid(), first.gid());
    fs::write(root.path().join("nested/alpha"), b"changed")?;
    assert_ne!(
        first.digest(),
        inspect_tree(root.path(), MaterializerLimits::PRODUCTION)?.digest()
    );
    Ok(())
}

#[test]
fn symlinks_are_counted_but_never_followed() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::write(outside.path().join("secret"), b"outside")?;
    symlink(outside.path(), root.path().join("link"))?;
    let proof = inspect_tree(root.path(), MaterializerLimits::PRODUCTION)?;
    assert_eq!(proof.safety(), TreeSafetyProof::new(1, 0, 0));
    let linked_root = root.path().with_extension("linked");
    symlink(root.path(), &linked_root)?;
    assert_eq!(
        inspect_tree(&linked_root, MaterializerLimits::PRODUCTION),
        Err(TreeError::RootSymlink)
    );
    Ok(())
}

#[test]
fn hardlinked_regular_paths_are_counted() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("first"), b"shared")?;
    fs::hard_link(root.path().join("first"), root.path().join("second"))?;
    let proof = inspect_tree(root.path(), MaterializerLimits::PRODUCTION)?;
    assert_eq!(proof.safety(), TreeSafetyProof::new(0, 0, 2));
    Ok(())
}

#[test]
fn unix_socket_is_counted_as_special_without_device_io() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let _listener = UnixListener::bind(root.path().join("socket"))?;
    let proof = inspect_tree(root.path(), MaterializerLimits::PRODUCTION)?;
    assert_eq!(proof.safety(), TreeSafetyProof::new(0, 1, 0));
    Ok(())
}

#[test]
fn depth_and_entry_bounds_fail_closed() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("one/two"))?;
    assert_eq!(
        inspect_tree(root.path(), limits(1, 10, 10, 10)?),
        Err(TreeError::DepthLimit)
    );

    let flat = tempfile::tempdir()?;
    fs::write(flat.path().join("one"), b"1")?;
    fs::write(flat.path().join("two"), b"2")?;
    assert_eq!(
        inspect_tree(flat.path(), limits(1, 2, 10, 10)?),
        Err(TreeError::EntryLimit)
    );
    Ok(())
}

#[test]
fn per_file_and_total_byte_bounds_fail_closed() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    fs::write(root.path().join("large"), b"1234")?;
    assert_eq!(
        inspect_tree(root.path(), limits(1, 2, 3, 10)?),
        Err(TreeError::FileSizeLimit)
    );

    let aggregate = tempfile::tempdir()?;
    fs::write(aggregate.path().join("one"), b"123")?;
    fs::write(aggregate.path().join("two"), b"456")?;
    assert_eq!(
        inspect_tree(aggregate.path(), limits(1, 3, 4, 5)?),
        Err(TreeError::TotalBytesLimit)
    );
    Ok(())
}

#[test]
fn sync_walks_clean_tree_and_rejects_unsafe_tree() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    fs::create_dir(root.path().join("nested"))?;
    fs::write(root.path().join("nested/file"), b"durable")?;
    sync_tree(root.path(), MaterializerLimits::PRODUCTION)?;

    symlink("nested/file", root.path().join("link"))?;
    assert_eq!(
        sync_tree(root.path(), MaterializerLimits::PRODUCTION),
        Err(TreeError::UnsafeTree)
    );
    assert_eq!(TreeError::UnsafeTree.code(), "unsafe_tree");
    Ok(())
}

#[test]
fn root_must_be_a_real_directory() -> Result<(), Box<dyn Error>> {
    let root = tempfile::tempdir()?;
    let file = root.path().join("file");
    fs::write(&file, b"not a directory")?;
    assert_eq!(
        inspect_tree(&file, MaterializerLimits::PRODUCTION),
        Err(TreeError::RootNotDirectory)
    );
    assert!(MaterializerLimits::bounded(65, 1, 1, 1).is_none());
    Ok(())
}
