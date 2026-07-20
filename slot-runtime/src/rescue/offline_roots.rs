use std::path::{Path, PathBuf};
use std::{fs, io};

use crate::domain::{PackageKey, UserId};
use crate::layout::RuntimeLayout;

#[derive(Debug, Clone)]
pub(super) struct RescueRoots {
    pub(super) management: PathBuf,
    pub(super) enrollment: PathBuf,
    pub(super) catalog: PathBuf,
    pub(super) journal: PathBuf,
}

impl RescueRoots {
    pub(super) fn fixed() -> Self {
        Self {
            management: RuntimeLayout::root().to_path_buf(),
            enrollment: RuntimeLayout::enrollment_root(),
            catalog: RuntimeLayout::catalog_root(),
            journal: RuntimeLayout::rescue_journal_root(),
        }
    }

    pub(super) fn with_roots(
        enrollment: impl AsRef<Path>,
        catalog: impl AsRef<Path>,
        journal: impl AsRef<Path>,
    ) -> Self {
        let enrollment = enrollment.as_ref().to_path_buf();
        let management = enrollment
            .parent()
            .map_or_else(|| enrollment.clone(), Path::to_path_buf);
        Self {
            management,
            enrollment,
            catalog: catalog.as_ref().to_path_buf(),
            journal: journal.as_ref().to_path_buf(),
        }
    }
}

pub(super) fn management_artifacts_present(
    roots: &RescueRoots,
    key: &PackageKey,
) -> Result<bool, crate::rescue::RescueError> {
    let package = key.package_name().as_str();
    let candidates = [
        roots.enrollment.join("packages").join(package),
        roots
            .enrollment
            .join("packages")
            .join(package)
            .join("enrollment.json"),
        roots.catalog.join("packages").join(package),
        roots
            .catalog
            .join("packages")
            .join(package)
            .join("slots/base.json"),
        roots.journal.join("packages").join(package),
        roots
            .management
            .join("registry")
            .join("packages")
            .join(package),
        roots
            .management
            .join("package-state")
            .join("packages")
            .join(package),
        roots
            .management
            .join("enrollment-attempts")
            .join("attempts")
            .join(package),
        roots
            .management
            .join("state")
            .join(format!("{package}.gate")),
        roots
            .management
            .join("state")
            .join(format!(".{package}.gate.retiring")),
        roots
            .management
            .join("state")
            .join(format!(".{package}.gate.retired")),
    ];
    for path in candidates {
        match fs::symlink_metadata(&path) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(crate::rescue::RescueError::io(
                    "inspect management artifact",
                    &path,
                    error,
                ));
            }
        }
    }
    Ok(false)
}

pub(super) fn supported(key: &PackageKey) -> bool {
    key.user_id() == UserId::PRIMARY
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "isolated temporary startup fixtures")]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::domain::PackageName;
    use crate::protocol::ALLOWED_PACKAGE;

    fn fixture() -> (TempDir, RescueRoots, PackageKey) {
        let root = TempDir::new().unwrap();
        let roots = RescueRoots::with_roots(
            root.path().join("enrollment"),
            root.path().join("catalog"),
            root.path().join("rescue-journal"),
        );
        let key = PackageKey::new(
            PackageName::parse(ALLOWED_PACKAGE).unwrap(),
            UserId::PRIMARY,
        );
        (root, roots, key)
    }

    #[test]
    fn empty_control_plane_is_not_managed() {
        let (_root, roots, key) = fixture();

        assert!(!management_artifacts_present(&roots, &key).unwrap());
    }

    #[test]
    fn enrollment_attempt_uses_the_real_attempts_directory() {
        let (root, roots, key) = fixture();
        fs::create_dir_all(
            root.path()
                .join("enrollment-attempts/attempts")
                .join(ALLOWED_PACKAGE),
        )
        .unwrap();

        assert!(management_artifacts_present(&roots, &key).unwrap());
    }

    #[test]
    fn malformed_enrollment_package_directory_is_still_managed() {
        let (root, roots, key) = fixture();
        fs::create_dir_all(
            root.path()
                .join("enrollment/packages")
                .join(ALLOWED_PACKAGE),
        )
        .unwrap();

        assert!(management_artifacts_present(&roots, &key).unwrap());
    }

    #[test]
    fn rescue_journal_epoch_is_a_management_artifact() {
        let (root, roots, key) = fixture();
        fs::create_dir_all(
            root.path()
                .join("rescue-journal/packages")
                .join(ALLOWED_PACKAGE)
                .join("rescue/steps"),
        )
        .unwrap();

        assert!(management_artifacts_present(&roots, &key).unwrap());
    }

    #[test]
    fn preliminary_gate_lease_is_a_management_artifact() {
        let (root, roots, key) = fixture();
        let state = root.path().join("state");
        fs::create_dir(&state).unwrap();
        fs::write(state.join(format!("{ALLOWED_PACKAGE}.gate")), b"prepared").unwrap();

        assert!(management_artifacts_present(&roots, &key).unwrap());
    }

    #[test]
    fn retiring_gate_lease_is_a_management_artifact() {
        let (root, roots, key) = fixture();
        let state = root.path().join("state");
        fs::create_dir(&state).unwrap();
        fs::write(
            state.join(format!(".{ALLOWED_PACKAGE}.gate.retiring")),
            b"retiring",
        )
        .unwrap();

        assert!(management_artifacts_present(&roots, &key).unwrap());
    }

    #[test]
    fn retired_gate_evidence_is_a_management_artifact() {
        let (root, roots, key) = fixture();
        let state = root.path().join("state");
        fs::create_dir(&state).unwrap();
        fs::write(
            state.join(format!(".{ALLOWED_PACKAGE}.gate.retired")),
            b"retired",
        )
        .unwrap();

        assert!(management_artifacts_present(&roots, &key).unwrap());
    }
}
