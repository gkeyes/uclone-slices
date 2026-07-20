use uclone_slot_runtime::android::{MountCounts, MountNamespaceProof};
use uclone_slot_runtime::domain::{AppIdentity, DataInodes, GateSnapshot, PackageEnabledState};
use uclone_slot_runtime::reconcile::NativeBaseRecoveryBackend;

use super::support::{DomainView, fixture as setup, managed};

fn assert_rejected_without_unmount(
    backend: &mut super::support::Backend,
    fixture: &super::support::Fixture,
    package: &uclone_slot_runtime::domain::ManagedPackage,
) {
    assert!(
        backend
            .restore_native_base_unconditionally(package)
            .is_err()
    );
    assert!(fixture.commands().is_empty());
}

#[test]
fn rejects_excess_mount_layers_without_unmounting() {
    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_counts(MountCounts::new(2, 1));

    assert_rejected_without_unmount(&mut backend, &fixture, &package);
}

#[test]
fn coherent_single_layer_source_is_rescued_without_catalog_dependency() {
    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    let unknown = DataInodes::new(505, 606).unwrap();
    fixture.set_view(unknown, unknown, unknown);

    backend
        .restore_native_base_unconditionally(&package)
        .unwrap();
    assert_eq!(fixture.commands().len(), 2);
    assert_eq!(fixture.view().canonical().inodes(), fixture.base());
}

#[test]
fn rejects_namespace_mirror_or_zygote_split_without_unmounting() {
    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_namespace(MountNamespaceProof::new(7, 8));
    assert_rejected_without_unmount(&mut backend, &fixture, &package);

    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_view(fixture.preview(), fixture.base(), fixture.preview());
    assert_rejected_without_unmount(&mut backend, &fixture, &package);

    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_view(fixture.preview(), fixture.preview(), fixture.base());
    assert_rejected_without_unmount(&mut backend, &fixture, &package);
}

#[test]
fn rejects_unheld_gate_or_live_process_without_unmounting() {
    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_gate(GateSnapshot::new(PackageEnabledState::Default, false));
    assert_rejected_without_unmount(&mut backend, &fixture, &package);

    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_processes(1);
    assert_rejected_without_unmount(&mut backend, &fixture, &package);

    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_user0_unlocked(false);
    assert_rejected_without_unmount(&mut backend, &fixture, &package);
}

#[test]
fn rejects_identity_pm_inode_or_install_drift_without_unmounting() {
    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_identity(
        AppIdentity::new(
            10_322,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            7,
            "/data/app/slotprobe/base.apk",
        )
        .unwrap(),
    );
    assert_rejected_without_unmount(&mut backend, &fixture, &package);

    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_package_manager(fixture.preview());
    assert_rejected_without_unmount(&mut backend, &fixture, &package);

    let (mut backend, fixture, package) = setup(DomainView::Preview, DomainView::Preview);
    fixture.set_pending_install(true);
    assert_rejected_without_unmount(&mut backend, &fixture, &package);
}

#[test]
fn accepts_any_valid_durably_managed_package() {
    let (mut backend, fixture, _) = setup(DomainView::Preview, DomainView::Preview);
    let package = managed("com.example.other");

    backend
        .restore_native_base_unconditionally(&package)
        .unwrap();
    assert_eq!(fixture.commands().len(), 2);
}
