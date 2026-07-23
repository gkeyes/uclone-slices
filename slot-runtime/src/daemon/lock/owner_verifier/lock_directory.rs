use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "android", target_os = "linux"))]
use std::os::fd::AsRawFd as _;

pub(super) struct PinnedLockDirectory {
    path: PathBuf,
    before: fs::Metadata,
    file: File,
}

impl PinnedLockDirectory {
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        let before = fs::symlink_metadata(path)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(no_follow_flag())
            .open(path)?;
        Ok(Self {
            path: path.to_path_buf(),
            before,
            file,
        })
    }

    pub(super) fn opened(&self) -> io::Result<fs::Metadata> {
        self.file.metadata()
    }

    pub(super) fn after(&self) -> io::Result<fs::Metadata> {
        fs::symlink_metadata(&self.path)
    }

    pub(super) fn owner_path(&self) -> PathBuf {
        #[cfg(any(target_os = "android", target_os = "linux"))]
        {
            PathBuf::from(format!("/proc/self/fd/{}/owner", self.file.as_raw_fd()))
        }
        #[cfg(target_os = "macos")]
        {
            self.path.join("owner")
        }
    }

    pub(super) fn same_as(&self, expected: &fs::Metadata, owner_uid: u32) -> io::Result<bool> {
        let opened = self.opened()?;
        Ok(same_directory_metadata(&self.before, expected, owner_uid)
            && same_directory_metadata(&opened, expected, owner_uid))
    }

    pub(super) fn same_after(&self, expected: &fs::Metadata, owner_uid: u32) -> io::Result<bool> {
        let opened = self.opened()?;
        let after = self.after()?;
        Ok(same_directory_metadata(&opened, expected, owner_uid)
            && same_directory_metadata(&after, expected, owner_uid))
    }
}

fn same_directory_metadata(
    current: &fs::Metadata,
    expected: &fs::Metadata,
    owner_uid: u32,
) -> bool {
    current.is_dir()
        && expected.is_dir()
        && current.uid() == owner_uid
        && expected.uid() == owner_uid
        && private_mode(current)
        && private_mode(expected)
        && current.dev() == expected.dev()
        && current.ino() == expected.ino()
}

fn private_mode(metadata: &fs::Metadata) -> bool {
    metadata.mode() & 0o777 == 0o700 && metadata.mode() & 0o7000 == 0
}

const fn no_follow_flag() -> i32 {
    #[cfg(target_os = "macos")]
    {
        0x0000_0100
    }
    #[cfg(not(target_os = "macos"))]
    {
        0x0002_0000
    }
}
