#![allow(
    clippy::unwrap_used,
    reason = "fixture setup must abort this unit test"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::TempDir;

use super::*;

#[test]
fn first_root_publication_syncs_its_parent_directory() {
    let parent = TempDir::new().unwrap();
    fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let root = parent.path().join("manifest-root");
    let before = DIRECTORY_SYNC_CALLS.with(std::cell::Cell::get);
    initialize_root(&root).unwrap();
    let after = DIRECTORY_SYNC_CALLS.with(std::cell::Cell::get);
    assert!(after > before);
}
