#![allow(
    clippy::unwrap_used,
    reason = "fixture construction must stop the focused test immediately"
)]

use std::fs;

use tempfile::TempDir;

use crate::domain::PackageName;
use crate::journal::JournalStore;
use crate::protocol::{Command, ErrorCode, Request, RequestId};
use crate::rescue::{NoRescueFault, OfflineRescuePlatform};

use super::authorize_direct_target;

#[derive(Debug)]
struct FakeBackend;

#[derive(Debug)]
struct FakeMetadata;

#[test]
fn direct_authorization_requires_existing_management_evidence() {
    let root = TempDir::new().unwrap();
    let enrollment = root.path().join("enrollment");
    let catalog = root.path().join("catalog");
    let journal = root.path().join("rescue-journal");
    let package = PackageName::parse("com.example.managed").unwrap();
    fs::create_dir_all(enrollment.join("packages").join(package.as_str())).unwrap();
    let mut platform = OfflineRescuePlatform::with_dependencies(
        FakeBackend,
        FakeMetadata,
        NoRescueFault,
        enrollment,
        catalog,
        journal,
    );

    assert_eq!(
        authorize_direct_target(&mut platform, &rescue(package)),
        Err(ErrorCode::NotFound)
    );
    assert_eq!(
        authorize_direct_target(
            &mut platform,
            &rescue(PackageName::parse("com.example.unmanaged").unwrap()),
        ),
        Err(ErrorCode::NotFound),
    );
    assert_eq!(
        authorize_direct_target(&mut platform, &probe()),
        Err(ErrorCode::InvalidRequest),
    );
}

#[test]
fn direct_authorization_maps_unattributed_journal_corruption_to_recovery_required() {
    let root = TempDir::new().unwrap();
    let enrollment = root.path().join("enrollment");
    let catalog = root.path().join("catalog");
    let rescue_journal = root.path().join("rescue-journal");
    let ordinary_journal = root.path().join("journal");
    JournalStore::new(&ordinary_journal).unwrap();
    fs::create_dir(ordinary_journal.join("transactions/corrupt-published")).unwrap();
    let package = PackageName::parse("com.example.arbitrary").unwrap();
    let mut platform = OfflineRescuePlatform::with_dependencies(
        FakeBackend,
        FakeMetadata,
        NoRescueFault,
        enrollment,
        catalog,
        rescue_journal,
    );

    assert_eq!(
        authorize_direct_target(&mut platform, &rescue(package)),
        Err(ErrorCode::RecoveryRequired),
    );
}

#[test]
fn weak_package_artifact_does_not_authorize_with_global_journal_corruption() {
    let root = TempDir::new().unwrap();
    let enrollment = root.path().join("enrollment");
    let catalog = root.path().join("catalog");
    let rescue_journal = root.path().join("rescue-journal");
    let ordinary_journal = root.path().join("journal");
    JournalStore::new(&ordinary_journal).unwrap();
    fs::create_dir(ordinary_journal.join("transactions/corrupt-published")).unwrap();
    let package = PackageName::parse("com.example.anchored").unwrap();
    fs::create_dir_all(enrollment.join("packages").join(package.as_str())).unwrap();
    fs::write(
        enrollment
            .join("packages")
            .join(package.as_str())
            .join("enrollment.json"),
        b"not-a-valid-enrollment",
    )
    .unwrap();
    let mut platform = OfflineRescuePlatform::with_dependencies(
        FakeBackend,
        FakeMetadata,
        NoRescueFault,
        enrollment,
        catalog,
        rescue_journal,
    );

    assert_eq!(
        authorize_direct_target(&mut platform, &rescue(package)),
        Err(ErrorCode::RecoveryRequired)
    );
}

#[test]
fn fake_management_artifacts_never_authorize_direct_rescue() {
    for (index, (case_name, relative)) in [
        ("dir", "enrollment/packages/com.example.fake"),
        (
            "enrollment",
            "enrollment/packages/com.example.fake/enrollment.json",
        ),
        (
            "catalog",
            "catalog/packages/com.example.fake/slots/base.json",
        ),
        (
            "revision",
            "registry/packages/com.example.fake/revisions/0000000000000001.json",
        ),
        ("lease", "state/com.example.fake.gate"),
    ]
    .into_iter()
    .enumerate()
    {
        let root = TempDir::new().unwrap();
        let path = root.path().join(relative);
        if index == 0 {
            fs::create_dir_all(&path).unwrap();
        } else {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"fake").unwrap();
        }
        let package = PackageName::parse("com.example.fake").unwrap();
        let mut platform = OfflineRescuePlatform::with_dependencies(
            FakeBackend,
            FakeMetadata,
            NoRescueFault,
            root.path().join("enrollment"),
            root.path().join("catalog"),
            root.path().join("rescue-journal"),
        );
        assert_eq!(
            authorize_direct_target(&mut platform, &rescue(package)),
            Err(ErrorCode::NotFound),
            "{case_name} must not authorize",
        );
    }
}

fn rescue(package: PackageName) -> Request {
    Request::new(
        RequestId::new("direct-rescue").unwrap(),
        Command::RescueToBase { package },
    )
    .unwrap()
}

fn probe() -> Request {
    Request::new(RequestId::new("direct-probe").unwrap(), Command::Probe).unwrap()
}
