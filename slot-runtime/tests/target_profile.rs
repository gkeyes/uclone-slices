#![doc = "Build-time target profile integration tests."]
#![allow(
    clippy::unwrap_used,
    reason = "fixed profile fixtures should abort the individual test on invalid generation"
)]

use std::path::Path;

use uclone_slot_runtime::domain::{PackageName, SlotId};
use uclone_slot_runtime::layout::RuntimeLayout;
use uclone_slot_runtime::protocol::{Command, ProtocolError, Request, RequestId};
use uclone_slot_runtime::target::{
    BASE_SLOT, CE_SLOT_ROOT, MODULE, PACKAGE, PREVIEW_CE, PREVIEW_DE, PREVIEW_SLOT, PROFILE,
    RUNTIME_ROOT, USER_ID,
};

fn request_id(value: &str) -> RequestId {
    RequestId::new(value).unwrap()
}

fn target_package() -> PackageName {
    PackageName::parse(PACKAGE).unwrap()
}

#[test]
fn compiled_constants_match_the_selected_fixed_profile() {
    match PROFILE {
        "slotprobe" => assert_eq!(PACKAGE, "com.uclone.slotprobe"),
        "fitness" => assert_eq!(PACKAGE, "com.asksky.fitness"),
        other => panic!("unexpected generated profile: {other}"),
    }
    assert_eq!(USER_ID, 0);
    assert_eq!(BASE_SLOT, "base");
    assert_eq!(PREVIEW_SLOT, "preview");
    assert_eq!(MODULE, "uclone-slices-preview");
    assert_eq!(RuntimeLayout::root(), Path::new(RUNTIME_ROOT));

    let preview = SlotId::parse(PREVIEW_SLOT).unwrap();
    let paths = RuntimeLayout::slot_paths(&target_package(), &preview);
    assert_eq!(paths.ce(), Path::new(PREVIEW_CE));
    assert_eq!(paths.de(), Path::new(PREVIEW_DE));
    assert!(PREVIEW_CE.starts_with(CE_SLOT_ROOT));
}

#[test]
fn protocol_accepts_only_the_compiled_package_and_slots() {
    let allowed = target_package();
    for slot in [SlotId::base(), SlotId::parse(PREVIEW_SLOT).unwrap()] {
        let request = Request::new(
            request_id("allowed"),
            Command::Switch {
                package: allowed.clone(),
                slot,
            },
        );
        assert!(request.is_ok());
    }

    let foreign_package = if PROFILE == "slotprobe" {
        "com.asksky.fitness"
    } else {
        "com.uclone.slotprobe"
    };
    let package_result = Request::new(
        request_id("foreign-package"),
        Command::StatusPackage {
            package: PackageName::parse(foreign_package).unwrap(),
        },
    );
    assert!(matches!(
        package_result,
        Err(ProtocolError::PackageNotAllowed(_))
    ));

    let slot_result = Request::new(
        request_id("foreign-slot"),
        Command::Switch {
            package: allowed,
            slot: SlotId::parse("foreign").unwrap(),
        },
    );
    assert!(matches!(slot_result, Err(ProtocolError::SlotNotAllowed(_))));
}
