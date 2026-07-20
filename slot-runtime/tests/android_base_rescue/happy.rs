use std::path::PathBuf;

use uclone_slot_runtime::android::{AndroidCommand, CommandKind, DataDomain, MountCounts};
use uclone_slot_runtime::domain::SlotId;
use uclone_slot_runtime::layout::RuntimeLayout;
use uclone_slot_runtime::reconcile::NativeBaseRecoveryBackend;

use super::support::{DomainView, fixture};

#[test]
fn native_base_is_idempotent_and_read_only() {
    let (mut backend, fixture, package) = fixture(DomainView::Base, DomainView::Base);

    backend
        .restore_native_base_unconditionally(&package)
        .unwrap();

    assert!(fixture.commands().is_empty());
    assert_eq!(
        fixture.view().canonical().mount_counts(),
        MountCounts::new(0, 0)
    );
}

#[test]
fn paired_preview_unmounts_only_fixed_canonical_ce_then_de() {
    let (mut backend, fixture, package) = fixture(DomainView::Preview, DomainView::Preview);
    let canonical = RuntimeLayout::slot_paths(package.package_name(), &SlotId::base());

    backend
        .restore_native_base_unconditionally(&package)
        .unwrap();

    let commands = fixture.commands();
    let kinds: Vec<_> = commands.iter().map(AndroidCommand::kind).collect();
    let targets: Vec<PathBuf> = commands
        .iter()
        .filter_map(|command| command.target().map(PathBuf::from))
        .collect();
    assert_eq!(
        kinds,
        [
            CommandKind::Unmount(DataDomain::Ce),
            CommandKind::Unmount(DataDomain::De),
        ]
    );
    assert_eq!(
        targets,
        [canonical.ce().to_path_buf(), canonical.de().to_path_buf()]
    );
    assert!(commands.iter().all(|command| command.source().is_none()));
    assert_eq!(fixture.view().canonical().inodes(), fixture.base());
    assert_eq!(
        fixture.view().canonical().mount_counts(),
        MountCounts::new(0, 0)
    );
}

#[test]
fn either_validated_hybrid_resumes_by_unmounting_only_the_remaining_domain() {
    for (ce, de, expected) in [
        (
            DomainView::Base,
            DomainView::Preview,
            CommandKind::Unmount(DataDomain::De),
        ),
        (
            DomainView::Preview,
            DomainView::Base,
            CommandKind::Unmount(DataDomain::Ce),
        ),
    ] {
        let (mut backend, fixture, package) = fixture(ce, de);

        backend
            .restore_native_base_unconditionally(&package)
            .unwrap();

        let kinds: Vec<_> = fixture
            .commands()
            .iter()
            .map(AndroidCommand::kind)
            .collect();
        assert_eq!(kinds, [expected]);
        assert_eq!(
            fixture.view().canonical().mount_counts(),
            MountCounts::new(0, 0)
        );
    }
}

#[test]
fn de_failure_leaves_a_disabled_resumable_hybrid() {
    let (mut backend, fixture, package) = fixture(DomainView::Preview, DomainView::Preview);
    fixture.fail_on(Some(DataDomain::De));

    let first = backend.restore_native_base_unconditionally(&package);

    assert!(first.is_err());
    assert_eq!(
        fixture.view().canonical().inodes().ce(),
        fixture.base().ce()
    );
    assert_eq!(
        fixture.view().canonical().inodes().de(),
        fixture.preview().de()
    );
    assert_eq!(
        fixture.view().canonical().mount_counts(),
        MountCounts::new(0, 1)
    );

    fixture.fail_on(None);
    backend
        .restore_native_base_unconditionally(&package)
        .unwrap();
    assert_eq!(fixture.view().canonical().inodes(), fixture.base());
    assert_eq!(
        fixture.view().canonical().mount_counts(),
        MountCounts::new(0, 0)
    );
}
