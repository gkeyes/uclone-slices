use std::path::Path;

use tempfile::TempDir;
use uclone_slot_runtime::catalog::CatalogStore;
use uclone_slot_runtime::materializer::{MaterializationCoordinator, NoFault};

use super::support::{Call, FakeBackend, managed_base, preview, security};

#[test]
fn publishes_preview_only_after_two_domain_verification_and_sync() {
    let mut backend = FakeBackend::healthy();
    let mut faults = NoFault;
    let result = MaterializationCoordinator::new(&mut backend, &mut faults)
        .materialize(&managed_base(), &preview())
        .unwrap();

    assert_eq!(result.slot_id(), &preview());
    assert_eq!(result.inodes().ce().get(), 300);
    assert_eq!(result.created_version_code(), 7);
    assert_eq!(
        backend.artifacts,
        uclone_slot_runtime::materializer::ArtifactState::ReadyOnly
    );
    assert_eq!(backend.calls.last(), Some(&Call::Publish));
    assert!(backend.calls.contains(&Call::Capacity));
    assert!(
        backend
            .calls
            .iter()
            .position(|call| *call == Call::Sync(uclone_slot_runtime::materializer::DataDomain::De))
            .unwrap()
            < backend
                .calls
                .iter()
                .position(|call| *call == Call::Publish)
                .unwrap()
    );
}

#[test]
fn result_maps_directly_to_catalog_fields_without_a_caller_path() {
    let package = managed_base();
    let mut backend = FakeBackend::healthy();
    let mut faults = NoFault;
    let result = MaterializationCoordinator::new(&mut backend, &mut faults)
        .materialize(&package, &preview())
        .unwrap();
    let root = TempDir::new().unwrap();
    let catalog = CatalogStore::new(root.path()).unwrap();
    catalog
        .create_base(
            result.package_key().clone(),
            package.base_inodes(),
            result.enrolled_identity().clone(),
            security(),
        )
        .unwrap();

    let entry = catalog
        .append_slot(
            result.package_key(),
            result.slot_id().clone(),
            result.inodes(),
            result.enrolled_identity(),
            result.created_version_code(),
            result.security_profile().clone(),
        )
        .unwrap();

    assert_eq!(entry.inodes(), result.inodes());
    assert_eq!(entry.slot_id(), result.slot_id());
}

#[test]
fn derives_all_paths_from_validated_package_and_fixed_preview_slot() {
    let mut backend = FakeBackend::healthy();
    let mut faults = NoFault;
    let result = MaterializationCoordinator::new(&mut backend, &mut faults)
        .materialize(&managed_base(), &preview())
        .unwrap();

    assert_eq!(
        result.paths().ready_ce(),
        Path::new("/data/misc_ce/0/uclone-slices-preview/slots/com.uclone.slotprobe/preview")
    );
    assert_eq!(
        result.paths().ready_de(),
        Path::new("/data/misc_de/0/uclone-slices-preview/slots/com.uclone.slotprobe/preview")
    );
    assert_eq!(
        result.paths().staging_ce().file_name().unwrap(),
        ".preview.staging"
    );
    assert_eq!(
        result.paths().staging_de().file_name().unwrap(),
        ".preview.staging"
    );
}
