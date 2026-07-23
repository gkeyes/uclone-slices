use std::fs;

use uclone_slot_runtime::domain::{
    AppIdentity, DataInodes, GateSnapshot, ManagedPackage, PackageEnabledState, PackageKey,
    PackageName, SlotId, SlotView, TransactionId, UserId,
};
use uclone_slot_runtime::journal::{JournalStore, TransactionSpec};
use uclone_slot_runtime::lifecycle::LifecycleState;
use uclone_slot_runtime::rescue::{
    RescueExecution, RescueStartup, RescueStatus, StartupGateOutcome,
};

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
        platform.startup_status(&Fixture::key()).unwrap(),
        Some(RescueStatus::BaseRetired)
    );
    let emergency_calls = platform.backend().emergency_gate_calls;

    assert_eq!(
        platform.reconcile_startup(&Fixture::key()),
        RescueStartup::BaseRetired
    );
    let (backend, _, _) = platform.into_dependencies();
    assert!(backend.native_base(fixture.managed()));
    assert!(!backend.gate_held());
    assert_eq!(backend.emergency_gate_calls, emergency_calls);
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

#[test]
fn corrupt_ordinary_journal_holds_package_gate_before_recovery_mode() {
    let fixture = Fixture::new();
    let journal_root = fixture.ordinary_root().parent().unwrap().join("journal");
    JournalStore::new(&journal_root).unwrap();
    std::fs::create_dir(journal_root.join("transactions/corrupt-published")).unwrap();
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );

    assert_eq!(
        platform.hold_startup_gate(&Fixture::key()).unwrap(),
        StartupGateOutcome::HeldRecovery
    );
    assert!(platform.backend().gate_held());
}

#[test]
fn attributed_journal_failure_holds_only_broken_package_and_keeps_peer_ordinary() {
    let fixture = Fixture::new();
    let root = fixture.ordinary_root().parent().unwrap();
    let journal_root = root.join("journal");
    let journal = JournalStore::new(&journal_root).unwrap();
    let broken = Fixture::key();
    let healthy = PackageKey::new(
        PackageName::parse("com.example.healthy").unwrap(),
        UserId::PRIMARY,
    );
    let broken_spec = ordinary_spec(broken.clone(), "broken-journal");
    let healthy_spec = ordinary_spec(healthy.clone(), "healthy-journal");
    journal.create(&broken_spec).unwrap();
    journal.create(&healthy_spec).unwrap();
    fs::write(
        journal.step_path(broken_spec.transaction_id(), 2).unwrap(),
        b"broken",
    )
    .unwrap();

    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    assert_eq!(
        platform.hold_startup_gate(&broken).unwrap(),
        StartupGateOutcome::HeldRecovery
    );
    assert_eq!(
        platform.hold_startup_gate(&healthy).unwrap(),
        StartupGateOutcome::Held
    );
    assert_eq!(platform.backend().emergency_gate_calls, 2);
}

fn ordinary_spec(key: PackageKey, transaction: &str) -> TransactionSpec {
    let base = DataInodes::new(901, 902).unwrap();
    let target = DataInodes::new(903, 904).unwrap();
    let identity = AppIdentity::new(
        10_321,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        1,
        "/data/app/test/base.apk",
    )
    .unwrap();
    let managed = ManagedPackage::new(
        key,
        identity,
        base,
        SlotView::new(SlotId::base(), base),
        LifecycleState::Normal,
    )
    .unwrap();
    TransactionSpec::new(
        TransactionId::parse(transaction).unwrap(),
        managed,
        SlotView::new(SlotId::parse("preview").unwrap(), target),
        GateSnapshot::new(PackageEnabledState::Default, false),
        "boot-startup-test",
    )
    .unwrap()
}
