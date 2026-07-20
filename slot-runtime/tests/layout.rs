#![doc = "Fixed Android Preview path derivation tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

use std::path::Path;

use uclone_slot_runtime::domain::{PackageName, SlotId};
use uclone_slot_runtime::layout::RuntimeLayout;

#[test]
fn derives_only_fixed_user_zero_slot_paths() {
    let package = PackageName::parse("com.uclone.slotprobe").unwrap();
    let slot = SlotId::parse("work").unwrap();

    let paths = RuntimeLayout::slot_paths(&package, &slot);

    assert_eq!(
        paths.ce(),
        Path::new("/data/misc_ce/0/uclone-slices-preview/slots/com.uclone.slotprobe/work")
    );
    assert_eq!(
        paths.de(),
        Path::new("/data/misc_de/0/uclone-slices-preview/slots/com.uclone.slotprobe/work")
    );
}

#[test]
fn resolves_base_to_android_canonical_paths() {
    let package = PackageName::parse("com.uclone.slotprobe").unwrap();

    let paths = RuntimeLayout::slot_paths(&package, &SlotId::base());

    assert_eq!(paths.ce(), Path::new("/data/user/0/com.uclone.slotprobe"));
    assert_eq!(
        paths.de(),
        Path::new("/data/user_de/0/com.uclone.slotprobe")
    );
}

#[test]
fn exposes_fixed_control_plane_paths() {
    assert_eq!(
        RuntimeLayout::root(),
        Path::new("/data/adb/uclone-slices-preview")
    );
    assert_eq!(
        RuntimeLayout::socket(),
        Path::new("/data/adb/uclone-slices-preview/run/ucloned.sock")
    );
    assert_eq!(
        RuntimeLayout::package_state_root(),
        Path::new("/data/adb/uclone-slices-preview/package-state")
    );
    assert_eq!(
        RuntimeLayout::catalog_root(),
        Path::new("/data/adb/uclone-slices-preview/catalog")
    );
    assert_eq!(
        RuntimeLayout::gate_state_root(),
        Path::new("/data/adb/uclone-slices-preview/state")
    );
}
