use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::io::Read as _;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use super::limits::{MaterializerLimits, TreeRootMetadata};
pub(super) use super::limits::{TreeError, TreeInspection};
use crate::materializer::TreeSafetyProof;

#[path = "tree_digest.rs"]
mod digest;
#[path = "tree_support.rs"]
mod support;
use digest::hash_metadata;
use support::{
    directory_path, ensure_path_and_file, ensure_path_metadata, ensure_same, open_nofollow,
    validate_open,
};

const READ_BUFFER_BYTES: usize = 64 * 1024;
#[cfg(any(target_os = "android", target_os = "linux"))]
const O_NOFOLLOW: i32 = 0x0002_0000;
#[cfg(any(target_os = "ios", target_os = "macos"))]
const O_NOFOLLOW: i32 = 0x0000_0100;

pub(super) fn inspect_tree(
    root: &Path,
    limits: MaterializerLimits,
) -> Result<TreeInspection, TreeError> {
    Walker::run(root, limits, false)
}

pub(super) fn sync_tree(root: &Path, limits: MaterializerLimits) -> Result<(), TreeError> {
    let expected = inspect_tree(root, limits)?;
    if !expected.is_clean() {
        return Err(TreeError::UnsafeTree);
    }
    let synchronized = Walker::run(root, limits, true)?;
    if synchronized == expected {
        Ok(())
    } else {
        Err(TreeError::MetadataChanged)
    }
}
struct Walker {
    limits: MaterializerLimits,
    entries: u64,
    total_bytes: u64,
    symlinks: u64,
    special: u64,
    hardlinks: u64,
    synchronize: bool,
    hasher: Sha256,
}
impl Walker {
    fn run(
        root: &Path,
        limits: MaterializerLimits,
        synchronize: bool,
    ) -> Result<TreeInspection, TreeError> {
        let root_metadata = fs::symlink_metadata(root).map_err(|_| TreeError::Io)?;
        if root_metadata.file_type().is_symlink() {
            return Err(TreeError::RootSymlink);
        }
        if !root_metadata.is_dir() {
            return Err(TreeError::RootNotDirectory);
        }
        validate_open(root, &root_metadata)?;
        let mut walker = Self {
            limits,
            entries: 0,
            total_bytes: 0,
            symlinks: 0,
            special: 0,
            hardlinks: 0,
            synchronize,
            hasher: Sha256::new(),
        };
        walker.hasher.update(b"uclone-materializer-tree-v1\0");
        walker.visit(root, Path::new(""), 0)?;
        ensure_path_metadata(root, &root_metadata)?;
        Ok(TreeInspection::new(
            format!("{:x}", walker.hasher.finalize()),
            TreeSafetyProof::new(walker.symlinks, walker.special, walker.hardlinks),
            TreeRootMetadata::new(
                root_metadata.dev(),
                root_metadata.ino(),
                root_metadata.uid(),
                root_metadata.gid(),
                root_metadata.mode() & 0o7777,
            ),
        ))
    }
    fn visit(&mut self, path: &Path, relative: &Path, depth: usize) -> Result<(), TreeError> {
        self.count_entry(depth)?;
        let metadata = fs::symlink_metadata(path).map_err(|_| TreeError::Io)?;
        let file_type = metadata.file_type();
        if file_type.is_dir() {
            hash_metadata(&mut self.hasher, relative, &metadata, b'd')?;
            self.visit_directory(path, relative, &metadata)
        } else if file_type.is_file() {
            self.visit_regular(path, relative, &metadata)
        } else {
            let tag = if file_type.is_symlink() { b'l' } else { b's' };
            hash_metadata(&mut self.hasher, relative, &metadata, tag)?;
            if file_type.is_symlink() {
                self.symlinks += 1;
            } else {
                self.special += 1;
            }
            ensure_path_metadata(path, &metadata)?;
            if self.synchronize {
                Err(TreeError::UnsafeTree)
            } else {
                Ok(())
            }
        }
    }
    fn count_entry(&mut self, depth: usize) -> Result<(), TreeError> {
        if depth > self.limits.max_depth() {
            return Err(TreeError::DepthLimit);
        }
        self.entries = self.entries.checked_add(1).ok_or(TreeError::EntryLimit)?;
        if self.entries > self.limits.max_entries() {
            Err(TreeError::EntryLimit)
        } else {
            Ok(())
        }
    }
    fn visit_directory(
        &mut self,
        path: &Path,
        relative: &Path,
        metadata: &Metadata,
    ) -> Result<(), TreeError> {
        let file = open_nofollow(path)?;
        ensure_same(metadata, &file.metadata().map_err(|_| TreeError::Io)?)?;
        let lookup = directory_path(&file, path);
        let names = self.read_names(&lookup)?;
        ensure_path_and_file(path, metadata, &file)?;
        for name in names {
            self.visit(
                &lookup.join(&name),
                &relative.join(name),
                Self::depth(relative)?,
            )?;
        }
        ensure_path_and_file(path, metadata, &file)?;
        if self.synchronize {
            file.sync_all().map_err(|_| TreeError::Io)?;
        }
        ensure_path_and_file(path, metadata, &file)
    }
    fn depth(relative: &Path) -> Result<usize, TreeError> {
        relative
            .components()
            .count()
            .checked_add(1)
            .ok_or(TreeError::DepthLimit)
    }
    fn read_names(&self, path: &Path) -> Result<Vec<OsString>, TreeError> {
        let mut names = Vec::new();
        for entry in fs::read_dir(path).map_err(|_| TreeError::Io)? {
            let current = u64::try_from(names.len()).map_err(|_| TreeError::EntryLimit)?;
            if current >= self.limits.max_entries().saturating_sub(self.entries) {
                return Err(TreeError::EntryLimit);
            }
            names.push(entry.map_err(|_| TreeError::Io)?.file_name());
        }
        names.sort_by(|left, right| {
            left.as_os_str()
                .as_bytes()
                .cmp(right.as_os_str().as_bytes())
        });
        Ok(names)
    }
    fn visit_regular(
        &mut self,
        path: &Path,
        relative: &Path,
        metadata: &Metadata,
    ) -> Result<(), TreeError> {
        let size = metadata.size();
        if size > self.limits.max_file_bytes() {
            return Err(TreeError::FileSizeLimit);
        }
        self.total_bytes = self
            .total_bytes
            .checked_add(size)
            .ok_or(TreeError::TotalBytesLimit)?;
        if self.total_bytes > self.limits.max_total_bytes() {
            return Err(TreeError::TotalBytesLimit);
        }
        if metadata.nlink() > 1 {
            self.hardlinks += 1;
            if self.synchronize {
                return Err(TreeError::UnsafeTree);
            }
        }
        let mut file = open_nofollow(path)?;
        ensure_same(metadata, &file.metadata().map_err(|_| TreeError::Io)?)?;
        hash_metadata(&mut self.hasher, relative, metadata, b'f')?;
        let observed = {
            let mut reader = (&mut file).take(size.saturating_add(1));
            let mut buffer = vec![0_u8; READ_BUFFER_BYTES].into_boxed_slice();
            let mut observed = 0_u64;
            loop {
                let count = reader.read(&mut buffer).map_err(|_| TreeError::Io)?;
                if count == 0 {
                    break;
                }
                let bytes = buffer.get(..count).ok_or(TreeError::Io)?;
                self.hasher.update(bytes);
                observed = observed
                    .checked_add(u64::try_from(count).map_err(|_| TreeError::Io)?)
                    .ok_or(TreeError::Io)?;
            }
            observed
        };
        if observed != size {
            return Err(TreeError::MetadataChanged);
        }
        ensure_path_and_file(path, metadata, &file)?;
        if self.synchronize {
            file.sync_all().map_err(|_| TreeError::Io)?;
        }
        ensure_path_and_file(path, metadata, &file)
    }
}
