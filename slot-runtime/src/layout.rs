#![doc = "Fixed path derivation for the user-zero Android Slots Preview."]

use std::path::{Path, PathBuf};

use crate::domain::{PackageName, SlotId};
use crate::target;

#[doc = "A paired CE/DE filesystem view derived from validated identifiers."]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotPaths {
    ce: PathBuf,
    de: PathBuf,
}

impl SlotPaths {
    #[doc = "Returns the CE directory for the view."]
    pub fn ce(&self) -> &Path {
        &self.ce
    }

    #[doc = "Returns the DE directory for the view."]
    pub fn de(&self) -> &Path {
        &self.de
    }
}

#[doc = "Namespace for the immutable Preview filesystem layout."]
#[derive(Debug, Clone, Copy)]
pub struct RuntimeLayout;

impl RuntimeLayout {
    #[doc = "Returns the root of the early-boot control plane."]
    pub fn root() -> &'static Path {
        Path::new(target::RUNTIME_ROOT)
    }

    #[doc = "Returns the Unix socket used by the root runtime."]
    pub fn socket() -> &'static Path {
        Path::new(target::RUNTIME_SOCKET)
    }

    #[doc = "Returns the fixed inter-process daemon/rescue mutex path."]
    pub fn lock() -> &'static Path {
        Path::new(target::RUNTIME_LOCK)
    }

    #[doc = "Returns the journal persistence root."]
    pub fn journal_root() -> PathBuf {
        Self::root().join("journal")
    }

    #[doc = "Returns the independent emergency native-base rescue journal root."]
    pub fn rescue_journal_root() -> PathBuf {
        Self::root().join("rescue-journal")
    }

    #[doc = "Returns the active-slot Registry persistence root."]
    pub fn registry_root() -> PathBuf {
        Self::root().join("registry")
    }

    #[doc = "Returns the immutable enrollment persistence root."]
    pub fn enrollment_root() -> PathBuf {
        Self::root().join("enrollment")
    }

    #[doc = "Returns the immutable compatibility acceptance root."]
    pub fn compatibility_policy_root() -> PathBuf {
        Self::root().join("compatibility-policy")
    }

    #[doc = "Returns the root-level enrollment attempt/recovery anchor directory."]
    pub fn enrollment_attempt_root() -> PathBuf {
        Self::root().join("enrollment-attempts")
    }

    #[doc = "Returns the append-only package lifecycle-state root."]
    pub fn package_state_root() -> PathBuf {
        Self::root().join("package-state")
    }

    #[doc = "Returns the immutable slot catalog root."]
    pub fn catalog_root() -> PathBuf {
        Self::root().join("catalog")
    }

    #[doc = "Returns the append-only slot display and lifecycle metadata root."]
    pub fn slot_metadata_root() -> PathBuf {
        Self::root().join("slot-metadata")
    }

    #[doc = "Returns the persisted execution-gate state directory."]
    pub fn gate_state_root() -> PathBuf {
        Self::root().join("state")
    }

    #[doc = "Derives CE and DE paths without accepting caller-controlled roots."]
    pub fn slot_paths(package_name: &PackageName, slot_id: &SlotId) -> SlotPaths {
        let package = package_name.as_str();
        if slot_id.is_base() {
            SlotPaths {
                ce: Path::new(target::CANONICAL_CE_ROOT).join(package),
                de: Path::new(target::CANONICAL_DE_ROOT).join(package),
            }
        } else {
            SlotPaths {
                ce: Path::new(target::CE_SLOT_ROOT)
                    .join(package)
                    .join(slot_id.as_str()),
                de: Path::new(target::DE_SLOT_ROOT)
                    .join(package)
                    .join(slot_id.as_str()),
            }
        }
    }
}
