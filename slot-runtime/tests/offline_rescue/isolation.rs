use uclone_slot_runtime::rescue::{RescueExecution, RescueStartup};

use super::support::{CrashOnce, FakeMetadata, Fixture};

#[test]
fn corrupt_ordinary_registry_journal_and_package_state_do_not_block_rescue() {
    let fixture = Fixture::new();
    fixture.corrupt_ordinary_stores();
    assert!(fixture.ordinary_root().exists());
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );

    assert_eq!(
        platform.rescue_to_base(&Fixture::key()),
        RescueExecution::CompletedBase
    );

    let (backend, _, _) = platform.into_dependencies();
    assert!(backend.native_base(fixture.managed()));
    assert!(!backend.lease_present());
}

#[test]
fn base_retired_startup_supersedes_corrupt_ordinary_control_plane() {
    let fixture = Fixture::new();
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    assert_eq!(
        platform.rescue_to_base(&Fixture::key()),
        RescueExecution::CompletedBase
    );
    fixture.corrupt_ordinary_stores();

    assert_eq!(
        platform.reconcile_startup(&Fixture::key()),
        RescueStartup::BaseRetired
    );
    let (backend, _, _) = platform.into_dependencies();
    assert!(backend.native_base(fixture.managed()));
    assert!(!backend.gate_held());
}

#[test]
fn ordinary_startup_is_gated_before_mutable_stores_are_opened() {
    let fixture = Fixture::new();
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );

    platform.hold_startup_gate(&Fixture::key()).unwrap();

    assert!(platform.backend().gate_held());
    assert!(platform.backend().lease_present());
    assert_eq!(platform.backend().emergency_gate_calls, 1);
}
