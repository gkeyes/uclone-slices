use super::support;

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};

use tempfile::TempDir;
use uclone_slot_runtime::domain::{CommitNonce, DataInodes, SlotId, SlotView};
use uclone_slot_runtime::registry::{PackageRevision, RegistryError, RegistryStore};

const OVERSIZED_PADDING: usize = 128 * 1024;

fn published(
    root: &Path,
) -> (
    RegistryStore,
    uclone_slot_runtime::domain::PackageName,
    PathBuf,
) {
    let store = RegistryStore::new(root).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let spec = support::transaction_spec(support::TransactionFixture::new(
        "tx-registry-storage-1",
        support::TransactionViews::new(
            base,
            SlotView::new(SlotId::base(), base),
            SlotView::new(
                SlotId::parse("work").unwrap(),
                DataInodes::new(300, 400).unwrap(),
            ),
        ),
        "boot-registry-storage",
    ));
    store
        .append(
            &PackageRevision::committed(
                &spec,
                base,
                CommitNonce::parse("nonce-registry-storage-1").unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let path = root.join("packages/com.uclone.slotprobe/revisions/0000000000000001.json");
    (store, spec.package_name().clone(), path)
}

#[test]
fn rejects_oversized_digest_valid_revision() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let (store, package, path) = published(&root.path().join("registry"));
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    let padding = vec![b' '; OVERSIZED_PADDING];
    file.write_all(&padding).unwrap();

    let error = store.latest(&package).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted Registry revision"));
}

#[test]
fn rejects_revision_with_permissive_mode() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let (store, package, path) = published(&root.path().join("registry"));
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();

    let error = store.latest(&package).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted Registry revision"));
}

#[test]
fn rejects_symlinked_revision_without_following_it() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let (store, package, path) = published(&root.path().join("registry"));
    let target = root.path().join("saved-revision.json");
    fs::rename(&path, &target).unwrap();
    symlink(target, path).unwrap();

    let error = store.latest(&package).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted Registry revision"));
}

#[test]
fn rejects_non_regular_revision() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let (store, package, path) = published(&root.path().join("registry"));
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();

    let error = store.latest(&package).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted Registry revision"));
}

#[test]
fn rejects_hard_linked_revision() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let (store, package, path) = published(&root.path().join("registry"));
    fs::hard_link(path, root.path().join("external-revision-link")).unwrap();

    let error = store.latest(&package).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted Registry revision"));
}

#[test]
fn rejects_unrecognized_hidden_registry_artifact() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let (store, package, path) = published(&root.path().join("registry"));
    fs::write(path.parent().unwrap().join(".attacker"), b"ignored").unwrap();

    let error = store.latest(&package).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("unexpected registry artifact"));
}

#[test]
fn hot_append_rejects_an_unrecognized_artifact_before_advancing() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let (store, _package, path) = published(&root.path().join("registry"));
    fs::write(path.parent().unwrap().join(".attacker"), b"ignored").unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let work = DataInodes::new(300, 400).unwrap();
    let spec = support::transaction_spec(support::TransactionFixture::new(
        "tx-registry-storage-2",
        support::TransactionViews::new(
            base,
            SlotView::new(SlotId::parse("work").unwrap(), work),
            SlotView::new(SlotId::base(), base),
        ),
        "boot-registry-storage",
    ));
    let draft = PackageRevision::committed(
        &spec,
        base,
        CommitNonce::parse("nonce-registry-storage-2").unwrap(),
    )
    .unwrap();

    let error = store.append(&draft).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("unexpected registry artifact"));
    assert!(
        !path
            .parent()
            .unwrap()
            .join("0000000000000002.json")
            .exists()
    );
}

#[test]
fn rejects_revisions_directory_with_permissive_mode() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let (store, package, path) = published(&root.path().join("registry"));
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o755)).unwrap();

    let error = store.latest(&package).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted Registry directory"));
}

#[test]
fn rejects_symlinked_packages_directory() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let target = root.path().join("redirected-packages");
    fs::create_dir(&target).unwrap();
    let registry = root.path().join("registry");
    fs::create_dir(&registry).unwrap();
    fs::set_permissions(&registry, fs::Permissions::from_mode(0o700)).unwrap();
    symlink(target, root.path().join("registry/packages")).unwrap();
    let error = RegistryStore::new(registry).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted Registry directory"));
}

#[test]
fn rejects_symlinked_registry_parent() {
    let root = TempDir::new().unwrap();
    support::secure_temp_dir(&root);
    let target = root.path().join("real-parent");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
    let linked_parent = root.path().join("linked-parent");
    symlink(target, &linked_parent).unwrap();

    let error = RegistryStore::new(linked_parent.join("registry")).unwrap_err();

    assert!(matches!(error, RegistryError::Corrupt(_)));
    assert!(error.to_string().contains("untrusted Registry directory"));
}
