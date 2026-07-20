#![doc = "Immutable package enrollment persistence tests."]
#![allow(
    clippy::unwrap_used,
    reason = "validated fixture construction must abort the individual test on failure"
)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use tempfile::TempDir;
use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, ManagedPackage, PackageKey, PackageName, SlotId, SlotView, UserId,
};
use uclone_slot_runtime::enrollment::EnrollmentStore;
use uclone_slot_runtime::lifecycle::LifecycleState;

#[path = "enrollment/storage_security.rs"]
mod storage_security;

const SIGNATURE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn managed(active: SlotView) -> ManagedPackage {
    let base = DataInodes::new(100, 200).unwrap();
    ManagedPackage::new(
        PackageKey::new(
            PackageName::parse("com.uclone.slotprobe").unwrap(),
            UserId::PRIMARY,
        ),
        AppIdentity::new(10_321, SIGNATURE, 1, "/data/app/slotprobe/base.apk").unwrap(),
        base,
        active,
        LifecycleState::Normal,
    )
    .unwrap()
}

fn secure_temp_dir() -> TempDir {
    let root = TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    root
}

#[test]
fn creates_and_loads_immutable_base_enrollment() {
    let root = secure_temp_dir();
    let store = EnrollmentStore::new(root.path()).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let enrolled = managed(SlotView::new(SlotId::base(), base));

    store.create(&enrolled).unwrap();

    let loaded = store.load(enrolled.package_name()).unwrap().unwrap();
    assert_eq!(loaded, enrolled);
}

#[test]
fn rejects_non_base_enrollment() {
    let root = secure_temp_dir();
    let store = EnrollmentStore::new(root.path()).unwrap();
    let work = SlotView::new(
        SlotId::parse("work").unwrap(),
        DataInodes::new(300, 400).unwrap(),
    );

    let error = store.create(&managed(work)).unwrap_err();

    assert!(error.to_string().contains("base"));
}

#[test]
fn rejects_tampered_enrollment_digest() {
    let root = secure_temp_dir();
    let store = EnrollmentStore::new(root.path()).unwrap();
    let base = DataInodes::new(100, 200).unwrap();
    let enrolled = managed(SlotView::new(SlotId::base(), base));
    store.create(&enrolled).unwrap();
    let path = root
        .path()
        .join("packages/com.uclone.slotprobe/enrollment.json");
    let mut bytes = fs::read(&path).unwrap();
    let marker = bytes.iter().position(|byte| *byte == b'a').unwrap();
    if let Some(byte) = bytes.get_mut(marker) {
        *byte = b'b';
    }
    fs::write(path, bytes).unwrap();

    let error = store.load(enrolled.package_name()).unwrap_err();

    assert!(error.to_string().contains("digest"));
}
