use uclone_slot_runtime::android::{CanonicalView, DataDomain, MountCounts, ViewProof};
use uclone_slot_runtime::domain::DataInodes;

use super::{DomainView, World};

pub(super) fn coherent(
    base: DataInodes,
    preview: DataInodes,
    ce: DomainView,
    de: DomainView,
) -> ViewProof {
    let inodes = pair(base, preview, ce, de);
    let counts = MountCounts::new(
        u32::from(matches!(ce, DomainView::Preview)),
        u32::from(matches!(de, DomainView::Preview)),
    );
    ViewProof::new(CanonicalView::new(inodes, counts), inodes, inodes)
}

fn pair(base: DataInodes, preview: DataInodes, ce: DomainView, de: DomainView) -> DataInodes {
    let ce = match ce {
        DomainView::Base => base.ce().get(),
        DomainView::Preview => preview.ce().get(),
    };
    let de = match de {
        DomainView::Base => base.de().get(),
        DomainView::Preview => preview.de().get(),
    };
    DataInodes::new(ce, de).unwrap()
}

pub(super) fn apply_unmount(world: &mut World, domain: DataDomain) {
    let current = world.view.canonical().inodes();
    let counts = world.view.canonical().mount_counts();
    let ce = if domain == DataDomain::Ce {
        world.base.ce().get()
    } else {
        current.ce().get()
    };
    let de = if domain == DataDomain::De {
        world.base.de().get()
    } else {
        current.de().get()
    };
    let inodes = DataInodes::new(ce, de).unwrap();
    let next_counts = MountCounts::new(
        if domain == DataDomain::Ce {
            0
        } else {
            counts.ce()
        },
        if domain == DataDomain::De {
            0
        } else {
            counts.de()
        },
    );
    world.view = ViewProof::new(CanonicalView::new(inodes, next_counts), inodes, inodes);
}
