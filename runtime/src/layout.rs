//! Where the runtime keeps its state and account data on the device. `kernelsu/service.sh`
//! repeats the runtime and storage roots; a test keeps them in step.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Package name of the Manager app.
pub const MANAGER_PACKAGE: &str = "com.uclone.slices.v2";

/// Default root of the runtime's protected state and its socket.
pub const RUNTIME_ROOT: &str = "/data/adb/uclone-slices-v2";
/// Default RPC socket of `ucloned`.
pub const RUNTIME_SOCKET: &str = "/data/adb/uclone-slices-v2/runtime.sock";

/// Credential-encrypted storage root of the runtime (user 0).
pub const CE_STORAGE_ROOT: &str = "/data/misc_ce/0/uclone-slices-v2";
/// Device-encrypted storage root of the runtime (user 0).
pub const DE_STORAGE_ROOT: &str = "/data/misc_de/0/uclone-slices-v2";
/// Credential-encrypted slot data, as `<root>/<package>/<slot>`.
pub const CE_SLOTS_ROOT: &str = "/data/misc_ce/0/uclone-slices-v2/slots";
/// Device-encrypted slot data, as `<root>/<package>/<slot>`.
pub const DE_SLOTS_ROOT: &str = "/data/misc_de/0/uclone-slices-v2/slots";
/// Credential-encrypted restore staging, as `<root>/<token>/<package>/<account>`.
pub const CE_TRANSFERS_ROOT: &str = "/data/misc_ce/0/uclone-slices-v2/transfers";
/// Device-encrypted restore staging, as `<root>/<token>/<package>/<account>`.
pub const DE_TRANSFERS_ROOT: &str = "/data/misc_de/0/uclone-slices-v2/transfers";

/// Credential-encrypted app data of user 0, as `<root>/<package>`.
pub const USER_CE_ROOT: &str = "/data/user/0";
/// Device-encrypted app data of user 0, as `<root>/<package>`.
pub const USER_DE_ROOT: &str = "/data/user_de/0";
/// Legacy alias of [`USER_CE_ROOT`].
pub const LEGACY_USER_CE_ROOT: &str = "/data/data";

/// Directory names below a storage root.
pub(crate) const TRANSFERS_DIR: &str = "transfers";
pub(crate) const MAINTENANCE_DIR: &str = "maintenance";
pub(crate) const ROLLBACK_DIR: &str = "rollback";
/// Slot data of unenrolled packages, as `<storage root>/trash/<unix seconds>[-N]/<package>`.
pub(crate) const TRASH_DIR: &str = "trash";
/// Below [`TRASH_DIR`]: data of an unenroll still in progress, which never expires.
pub(crate) const TRASH_PENDING_DIR: &str = "pending";

/// How long unenrolled slot data stays in the trash before it is deleted for good.
pub(crate) const TRASH_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Mount roots in `/proc/self/mountinfo` that contain these markers come from the runtime.
pub(crate) const SLOTS_MOUNT_MARKER: &str = "/uclone-slices-v2/slots/";
pub(crate) const MAINTENANCE_MOUNT_MARKER: &str = "/uclone-slices-v2/maintenance/";

/// The Manager's cache directories, the only place the archive helper reads or writes archives.
pub fn manager_cache_roots() -> [PathBuf; 2] {
    [USER_CE_ROOT, LEGACY_USER_CE_ROOT]
        .map(|root| Path::new(root).join(MANAGER_PACKAGE).join("cache"))
}

/// `<storage root>/<area>/<parts...>` for both domains, where each storage root is the parent of
/// the given slots root.
pub(crate) fn storage_paths(
    ce_slots_root: &Path,
    de_slots_root: &Path,
    area: &str,
    parts: &[&str],
) -> Option<(PathBuf, PathBuf)> {
    let build = |slots_root: &Path| {
        let root = slots_root.parent()?.join(area);
        Some(parts.iter().fold(root, |path, part| path.join(part)))
    };
    Some((build(ce_slots_root)?, build(de_slots_root)?))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn roots_and_markers_agree() {
        for (storage, slots, transfers) in [
            (CE_STORAGE_ROOT, CE_SLOTS_ROOT, CE_TRANSFERS_ROOT),
            (DE_STORAGE_ROOT, DE_SLOTS_ROOT, DE_TRANSFERS_ROOT),
        ] {
            assert_eq!(Path::new(storage).join("slots"), Path::new(slots));
            assert_eq!(Path::new(storage).join(TRANSFERS_DIR), Path::new(transfers));
            assert!(format!("{slots}/").ends_with(SLOTS_MOUNT_MARKER));
            assert!(format!("{storage}/{MAINTENANCE_DIR}/").ends_with(MAINTENANCE_MOUNT_MARKER));
        }
    }

    #[test]
    fn storage_paths_sit_beside_the_slots_root() {
        assert_eq!(
            storage_paths(
                Path::new(CE_SLOTS_ROOT),
                Path::new(DE_SLOTS_ROOT),
                MAINTENANCE_DIR,
                &["token", "com.example.app"],
            ),
            Some((
                PathBuf::from("/data/misc_ce/0/uclone-slices-v2/maintenance/token/com.example.app"),
                PathBuf::from("/data/misc_de/0/uclone-slices-v2/maintenance/token/com.example.app"),
            ))
        );
        assert_eq!(
            storage_paths(Path::new("/"), Path::new(DE_SLOTS_ROOT), TRANSFERS_DIR, &[]),
            None
        );
    }

    #[test]
    fn service_script_uses_the_same_storage_roots() {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../kernelsu/service.sh");
        let script = std::fs::read_to_string(script).unwrap();
        assert!(script.contains(&format!("RUNTIME_ROOT={RUNTIME_ROOT}\n")));
        assert!(script.contains("SOCKET=$RUNTIME_ROOT/runtime.sock\n"));
        assert_eq!(
            Path::new(RUNTIME_ROOT).join("runtime.sock"),
            Path::new(RUNTIME_SOCKET)
        );
        assert!(script.contains(&format!("CE_STORAGE_ROOT={CE_STORAGE_ROOT}\n")));
        assert!(script.contains(&format!("DE_STORAGE_ROOT={DE_STORAGE_ROOT}\n")));
    }
}
