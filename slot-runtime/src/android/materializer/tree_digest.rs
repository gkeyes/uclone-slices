use std::fs::Metadata;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use super::TreeError;

pub(super) fn hash_metadata(
    hasher: &mut Sha256,
    relative: &Path,
    metadata: &Metadata,
    tag: u8,
) -> Result<(), TreeError> {
    let name = relative.as_os_str().as_bytes();
    hasher.update(
        u64::try_from(name.len())
            .map_err(|_| TreeError::Io)?
            .to_be_bytes(),
    );
    hasher.update(name);
    hasher.update([tag]);
    hasher.update(metadata.uid().to_be_bytes());
    hasher.update(metadata.gid().to_be_bytes());
    hasher.update((metadata.mode() & 0o7777).to_be_bytes());
    hasher.update(metadata_size_for_digest(metadata, tag).to_be_bytes());
    Ok(())
}

pub(super) fn metadata_size_for_digest(metadata: &Metadata, tag: u8) -> u64 {
    if tag == b'f' && metadata.file_type().is_file() {
        metadata.size()
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::metadata_size_for_digest;
    use std::error::Error;
    use std::fs;

    #[test]
    fn directory_metadata_size_is_not_part_of_logical_tree_digest() -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let metadata = fs::symlink_metadata(root.path())?;

        assert_eq!(metadata_size_for_digest(&metadata, b'd'), 0);
        Ok(())
    }

    #[test]
    fn regular_file_size_is_part_of_logical_tree_digest() -> Result<(), Box<dyn Error>> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("payload");
        fs::write(&path, b"payload")?;
        let metadata = fs::symlink_metadata(path)?;

        assert_eq!(metadata_size_for_digest(&metadata, b'f'), 7);
        Ok(())
    }
}
