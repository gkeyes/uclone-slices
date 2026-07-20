use crate::materializer::TreeSafetyProof;

const HARD_MAX_DEPTH: usize = 64;
const HARD_MAX_ENTRIES: u64 = 250_000;
const HARD_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const HARD_MAX_TOTAL_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[doc = "Fixed resource bounds for one Android materializer tree walk."]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterializerLimits {
    depth: usize,
    entries: u64,
    file_bytes: u64,
    total_bytes: u64,
}

impl MaterializerLimits {
    #[doc = "Production bounds, each fixed at its compiled hard cap."]
    pub const PRODUCTION: Self = Self {
        depth: HARD_MAX_DEPTH,
        entries: HARD_MAX_ENTRIES,
        file_bytes: HARD_MAX_FILE_BYTES,
        total_bytes: HARD_MAX_TOTAL_BYTES,
    };

    #[doc = "Builds smaller bounds only when none exceeds a production hard cap."]
    pub const fn bounded(
        max_depth: usize,
        max_entries: u64,
        max_file_bytes: u64,
        max_total_bytes: u64,
    ) -> Option<Self> {
        if max_depth <= HARD_MAX_DEPTH
            && max_entries <= HARD_MAX_ENTRIES
            && max_file_bytes <= HARD_MAX_FILE_BYTES
            && max_total_bytes <= HARD_MAX_TOTAL_BYTES
        {
            Some(Self {
                depth: max_depth,
                entries: max_entries,
                file_bytes: max_file_bytes,
                total_bytes: max_total_bytes,
            })
        } else {
            None
        }
    }

    #[doc = "Returns the maximum relative depth, with the root at depth zero."]
    pub(super) const fn max_depth(self) -> usize {
        self.depth
    }

    #[doc = "Returns the maximum number of entries, including the root."]
    pub(super) const fn max_entries(self) -> u64 {
        self.entries
    }

    #[doc = "Returns the maximum bytes read from any one regular file."]
    pub(super) const fn max_file_bytes(self) -> u64 {
        self.file_bytes
    }

    #[doc = "Returns the maximum aggregate bytes read from regular files."]
    pub(super) const fn max_total_bytes(self) -> u64 {
        self.total_bytes
    }
}

impl Default for MaterializerLimits {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

#[doc = "Bounded deterministic evidence for one materializer directory tree."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TreeInspection {
    digest: String,
    safety: TreeSafetyProof,
    device_id: u64,
    inode: u64,
    uid: u32,
    gid: u32,
    mode: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TreeRootMetadata {
    pub(super) device_id: u64,
    pub(super) inode: u64,
    pub(super) uid: u32,
    pub(super) gid: u32,
    pub(super) mode: u32,
}

impl TreeRootMetadata {
    pub(super) const fn new(device_id: u64, inode: u64, uid: u32, gid: u32, mode: u32) -> Self {
        Self {
            device_id,
            inode,
            uid,
            gid,
            mode,
        }
    }
}

impl TreeInspection {
    pub(super) const fn new(
        digest: String,
        safety: TreeSafetyProof,
        root: TreeRootMetadata,
    ) -> Self {
        Self {
            digest,
            safety,
            device_id: root.device_id,
            inode: root.inode,
            uid: root.uid,
            gid: root.gid,
            mode: root.mode,
        }
    }

    #[doc = "Returns the deterministic SHA-256 tree digest."]
    pub(super) fn digest(&self) -> &str {
        &self.digest
    }

    #[doc = "Returns exact forbidden-artifact counters."]
    pub(super) const fn safety(&self) -> TreeSafetyProof {
        self.safety
    }

    #[doc = "Returns the root filesystem device identifier."]
    pub(super) const fn device_id(&self) -> u64 {
        self.device_id
    }

    #[doc = "Returns the root inode number."]
    pub(super) const fn inode(&self) -> u64 {
        self.inode
    }

    #[doc = "Returns the root Unix owner identifier."]
    pub(super) const fn uid(&self) -> u32 {
        self.uid
    }

    #[doc = "Returns the root Unix group identifier."]
    pub(super) const fn gid(&self) -> u32 {
        self.gid
    }

    #[doc = "Returns the root permission and special mode bits."]
    pub(super) const fn mode(&self) -> u32 {
        self.mode
    }

    pub(super) fn is_clean(&self) -> bool {
        self.safety == TreeSafetyProof::clean()
    }
}

#[doc = "Stable fail-closed reason from a bounded tree operation."]
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(super) enum TreeError {
    #[doc = "The supplied root is a symbolic link."]
    #[error("tree root is a symbolic link")]
    RootSymlink,
    #[doc = "The supplied root is not a directory."]
    #[error("tree root is not a directory")]
    RootNotDirectory,
    #[doc = "A path exceeded the maximum relative depth."]
    #[error("tree depth exceeded its bound")]
    DepthLimit,
    #[doc = "The tree exceeded the maximum entry count."]
    #[error("tree entry count exceeded its bound")]
    EntryLimit,
    #[doc = "A regular file exceeded the per-file byte bound."]
    #[error("regular file exceeded its byte bound")]
    FileSizeLimit,
    #[doc = "Regular files exceeded the aggregate byte bound."]
    #[error("tree bytes exceeded their aggregate bound")]
    TotalBytesLimit,
    #[doc = "Filesystem metadata changed during an observation."]
    #[error("tree metadata changed during observation")]
    MetadataChanged,
    #[doc = "A durability pass encountered a forbidden artifact."]
    #[error("tree became unsafe before durability synchronization")]
    UnsafeTree,
    #[doc = "A filesystem operation failed without exposing its path."]
    #[error("tree filesystem operation failed")]
    Io,
}

impl TreeError {
    #[doc = "Returns a bounded diagnostic code without a filesystem path."]
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::RootSymlink => "root_symlink",
            Self::RootNotDirectory => "root_not_directory",
            Self::DepthLimit => "depth_limit",
            Self::EntryLimit => "entry_limit",
            Self::FileSizeLimit => "file_size_limit",
            Self::TotalBytesLimit => "total_bytes_limit",
            Self::MetadataChanged => "metadata_changed",
            Self::UnsafeTree => "unsafe_tree",
            Self::Io => "io",
        }
    }
}
