use std::fs;
use std::os::unix::fs::PermissionsExt as _;

use uclone_slot_runtime::daemon::RequestHandler;
use uclone_slot_runtime::domain::PackageName;
use uclone_slot_runtime::journal::JournalStore;
use uclone_slot_runtime::protocol::{
    AckOperation, Command, ErrorCode, Request, RequestId, ResponsePayload,
};
use uclone_slot_runtime::rescue::StartupGateOutcome;
use uclone_slot_runtime::service::PreviewService;

use super::support::{CrashOnce, FakeMetadata, Fixture};

#[test]
fn recovery_only_service_lists_discovered_targets_without_ordinary_state() {
    let fixture = Fixture::new();
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    assert!(platform.authorize_existing_target(&Fixture::key()).unwrap());
    let mut service = PreviewService::new_recovery_only(platform);

    let probe = service.handle(&request(Command::Probe));
    assert!(matches!(
        probe.payload(),
        Some(ResponsePayload::ProbeReport(report)) if report.recovery_only()
    ));

    let listed = service.handle(&request(Command::ListRecoveryTargets));
    assert!(matches!(
        listed.payload(),
        Some(ResponsePayload::RecoveryTargets(report))
            if report.targets() == [Fixture::key().package_name().clone()]
    ));

    let status = service.handle(&request(Command::StatusPackage {
        package: Fixture::key().package_name().clone(),
    }));
    assert_eq!(status.error_code(), Some(ErrorCode::RecoveryRequired));
}

#[test]
fn unknown_package_cannot_use_recovery_only_rescue() {
    let fixture = Fixture::new();
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    assert!(platform.authorize_existing_target(&Fixture::key()).unwrap());
    let mut service = PreviewService::new_recovery_only(platform);
    let unknown = PackageName::parse("com.example.unknown").unwrap();

    let rescued = service.handle(&request(Command::RescueToBase { package: unknown }));

    assert_eq!(rescued.error_code(), Some(ErrorCode::NotFound));
}

#[test]
fn recovery_only_service_rejects_every_non_allowlisted_alias_without_gating() {
    let fixture = Fixture::new();
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    assert!(platform.authorize_existing_target(&Fixture::key()).unwrap());
    let mut service = PreviewService::new_recovery_only(platform);

    for command in [
        Command::InspectPackage {
            package: Fixture::key().package_name().clone(),
        },
        Command::ListManagedApps,
        Command::StatusPackage {
            package: Fixture::key().package_name().clone(),
        },
        Command::ListSlots {
            package: Fixture::key().package_name().clone(),
        },
        Command::Reconcile,
        Command::ReconcilePackage {
            package: Fixture::key().package_name().clone(),
        },
        Command::RetirePackage {
            package: Fixture::key().package_name().clone(),
        },
    ] {
        let response = service.handle(&request(command));
        assert_eq!(response.error_code(), Some(ErrorCode::RecoveryRequired));
    }

    assert_eq!(service.platform().backend().emergency_gate_calls, 0);
}

#[test]
fn direct_rescue_authorizes_only_a_target_with_management_evidence() {
    let fixture = Fixture::new();
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    let unknown = uclone_slot_runtime::domain::PackageKey::new(
        PackageName::parse("com.example.unknown").unwrap(),
        uclone_slot_runtime::domain::UserId::PRIMARY,
    );

    assert!(!platform.authorize_existing_target(&unknown).unwrap());
    assert_eq!(platform.backend().emergency_gate_calls, 0);
    assert!(platform.authorize_existing_target(&Fixture::key()).unwrap());

    let mut service = PreviewService::new_recovery_only(platform);
    let rescued = service.handle(&request(Command::RescueToBase {
        package: Fixture::key().package_name().clone(),
    }));

    assert!(matches!(
        rescued.payload(),
        Some(ResponsePayload::Ack(ack)) if ack.operation() == AckOperation::RescueToBase
    ));
}

#[test]
fn weak_management_artifact_is_not_direct_rescue_authority() {
    let fixture = Fixture::new();
    let weak = PackageName::parse("com.example.weakartifact").unwrap();
    let root = fixture.ordinary_root().parent().unwrap();
    fs::create_dir_all(
        root.join("compatibility-policy/packages")
            .join(weak.as_str()),
    )
    .unwrap();
    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    let key = uclone_slot_runtime::domain::PackageKey::new(
        weak.clone(),
        uclone_slot_runtime::domain::UserId::PRIMARY,
    );

    assert!(!platform.authorize_existing_target(&key).unwrap());
    let mut service = PreviewService::new_recovery_only(platform);
    let listed = service.handle(&request(Command::ListRecoveryTargets));
    assert!(matches!(
        listed.payload(),
        Some(ResponsePayload::RecoveryTargets(report)) if report.targets().is_empty()
    ));
    let rescued = service.handle(&request(Command::RescueToBase { package: weak }));
    assert_eq!(rescued.error_code(), Some(ErrorCode::NotFound));
    assert_eq!(service.platform().backend().emergency_gate_calls, 0);

    let mut startup = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    assert_eq!(
        startup.hold_startup_gate(&key).unwrap(),
        StartupGateOutcome::Held
    );
    assert_eq!(startup.backend().emergency_gate_calls, 1);
}

#[test]
fn valid_rescue_journal_authorizes_target_when_ordinary_journal_is_corrupt() {
    let fixture = Fixture::new();
    let mut first = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    assert_eq!(
        first.rescue_to_base(&Fixture::key()),
        uclone_slot_runtime::rescue::RescueExecution::CompletedBase
    );
    let root = fixture.ordinary_root().parent().unwrap();
    fs::write(
        root.join("enrollment/packages/com.uclone.slotprobe/enrollment.json"),
        b"corrupt",
    )
    .unwrap();
    fs::write(
        root.join("catalog/packages/com.uclone.slotprobe/slots/base.json"),
        b"corrupt",
    )
    .unwrap();
    let ordinary_root = fixture.ordinary_root().parent().unwrap().join("journal");
    JournalStore::new(&ordinary_root).unwrap();
    std::fs::create_dir(ordinary_root.join("transactions/corrupt-published")).unwrap();

    let mut platform = fixture.platform(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
    );
    assert!(platform.authorize_existing_target(&Fixture::key()).unwrap());
    assert_eq!(platform.backend().emergency_gate_calls, 0);
}

#[test]
fn unattributed_journal_corruption_cannot_authorize_or_gate_an_arbitrary_target() {
    let fixture = Fixture::new();
    let root = tempfile::TempDir::new().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let journal_root = root.path().join("journal");
    JournalStore::new(&journal_root).unwrap();
    std::fs::create_dir(journal_root.join("transactions/corrupt-published")).unwrap();
    let mut platform = uclone_slot_runtime::rescue::OfflineRescuePlatform::with_dependencies(
        fixture.backend(),
        FakeMetadata::default(),
        CrashOnce::never(),
        root.path().join("enrollment"),
        root.path().join("catalog"),
        root.path().join("rescue-journal"),
    );
    let arbitrary = uclone_slot_runtime::domain::PackageKey::new(
        PackageName::parse("com.example.arbitrary").unwrap(),
        uclone_slot_runtime::domain::UserId::PRIMARY,
    );

    assert!(platform.authorize_existing_target(&arbitrary).is_err());
    assert_eq!(platform.backend().emergency_gate_calls, 0);
}

fn request(command: Command) -> Request {
    Request::new(RequestId::new("recovery-targets").unwrap(), command).unwrap()
}
