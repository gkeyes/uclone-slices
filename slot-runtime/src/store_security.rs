#![allow(
    clippy::redundant_pub_crate,
    reason = "shared only by sibling persistence modules while this helper stays internal"
)]

use std::io;

mod file;
mod shape;

pub(crate) use file::{read_record, validate_record, write_new_record};
pub(crate) use shape::{
    ensure_child_directory, initialize_root, sync_directory, validate_directory,
};

const DIRECTORY_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;
const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_RECORD_BYTES_U64: u64 = 64 * 1024;

#[derive(Debug)]
pub(crate) enum StoreSecurityError {
    Io(io::Error),
    Corrupt(String),
}

pub(super) const fn no_follow_flag() -> i32 {
    #[cfg(target_os = "macos")]
    {
        0x0000_0100
    }
    #[cfg(not(target_os = "macos"))]
    {
        0x0002_0000
    }
}
