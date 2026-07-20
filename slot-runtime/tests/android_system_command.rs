#![doc = "Fixed Android system command construction tests."]
#![allow(
    clippy::redundant_pub_crate,
    clippy::unwrap_used,
    reason = "included production module keeps its original crate-relative visibility"
)]
#![allow(
    unreachable_pub,
    reason = "production module is included as a test fixture"
)]

use std::ffi::OsStr;
use std::path::Path;

pub use uclone_slot_runtime::{android, bridge, domain, layout};

#[path = "../src/android/system/mod.rs"]
mod system;

use android::DataDomain;
use domain::{PackageName, SlotId};
use layout::RuntimeLayout;

fn arguments(invocation: &system::ProcessInvocation) -> Vec<&OsStr> {
    invocation.args().iter().map(AsRef::as_ref).collect()
}

#[test]
fn force_stop_uses_only_fixed_am_argv() {
    let package = PackageName::parse("com.uclone.slotprobe").unwrap();
    let invocation = system::force_stop_invocation(&package);

    assert_eq!(invocation.program(), "/system/bin/am");
    assert_eq!(
        arguments(&invocation),
        ["force-stop", "--user", "0", "com.uclone.slotprobe"].map(OsStr::new)
    );
    assert_eq!(invocation.timeout().as_secs(), 5);
    assert_eq!(invocation.output_limit(), 16 * 1024);
}

#[test]
fn bind_argv_accepts_only_runtime_layout_source_and_target() {
    let package = PackageName::parse("com.uclone.slotprobe").unwrap();
    let slot = SlotId::parse("preview").unwrap();
    let source = RuntimeLayout::slot_paths(&package, &slot);
    let target = RuntimeLayout::slot_paths(&package, &SlotId::base());

    let ce = system::bind_invocation_for_paths(&package, DataDomain::Ce, source.ce(), target.ce())
        .unwrap();
    assert_eq!(ce.program(), "/system/bin/mount");
    assert_eq!(
        arguments(&ce),
        [
            OsStr::new("--bind"),
            source.ce().as_os_str(),
            target.ce().as_os_str()
        ]
    );

    let de = system::bind_invocation_for_paths(&package, DataDomain::De, source.de(), target.de())
        .unwrap();
    assert_eq!(
        arguments(&de),
        [
            OsStr::new("--bind"),
            source.de().as_os_str(),
            target.de().as_os_str()
        ]
    );

    assert!(
        system::bind_invocation_for_paths(
            &package,
            DataDomain::Ce,
            Path::new("/data/local/tmp/caller"),
            target.ce(),
        )
        .is_err()
    );
    assert!(
        system::bind_invocation_for_paths(&package, DataDomain::Ce, source.ce(), target.de(),)
            .is_err()
    );
}

#[test]
fn unmount_and_namespace_stat_are_fixed_typed_argv() {
    let package = PackageName::parse("com.uclone.slotprobe").unwrap();
    let base = RuntimeLayout::slot_paths(&package, &SlotId::base());
    let unmount = system::unmount_invocation_for_target(base.ce());
    assert_eq!(unmount.program(), "/system/bin/umount");
    assert_eq!(arguments(&unmount), [base.ce().as_os_str()]);

    let stat = system::namespace_stat_invocation(4321, base.ce(), base.de());
    assert_eq!(stat.program(), "/system/bin/nsenter");
    assert_eq!(
        arguments(&stat),
        [
            OsStr::new("-t"),
            OsStr::new("4321"),
            OsStr::new("-m"),
            OsStr::new("--"),
            OsStr::new("/system/bin/stat"),
            OsStr::new("-L"),
            OsStr::new("-c"),
            OsStr::new("%i"),
            OsStr::new("--"),
            base.ce().as_os_str(),
            base.de().as_os_str(),
        ]
    );
}
