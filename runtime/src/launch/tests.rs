#![allow(clippy::unwrap_used)]

use super::*;
use crate::model::{PackageIdentity, SlotId};

fn stored_package(root: &Path) -> PackageAggregate {
    let package = PackageName::new("com.example.app").unwrap();
    let identity = PackageIdentity::new(10123, "/data/app/example/base.apk", 1, 2).unwrap();
    let mut aggregate = PackageAggregate::enrolled(package, identity);
    aggregate.set_default_slot(Some(SlotId::base())).unwrap();
    FilePackageStore::open(root)
        .unwrap()
        .save(&aggregate)
        .unwrap();
    aggregate
}

#[test]
fn policy_is_derived_from_the_aggregate_including_a_closed_default() {
    let root = tempfile::tempdir().unwrap();
    let mut aggregate = stored_package(root.path());
    let first = load_policies(root.path()).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].uid, 10123);
    assert_eq!(first[0].default.as_deref(), Some("base"));
    aggregate.set_default_slot(None).unwrap();
    FilePackageStore::open(root.path())
        .unwrap()
        .save(&aggregate)
        .unwrap();
    assert_eq!(load_policies(root.path()).unwrap()[0].default, None);
}

#[test]
fn guessed_or_cancelled_tickets_cannot_authorize_account_preparation() {
    let root = tempfile::tempdir().unwrap();
    let gate = LaunchGate::new(root.path());
    assert!(!gate.ticket_live(&"a".repeat(64)));
    let (reader, writer) = UnixStream::pair().unwrap();
    let token = "b".repeat(64);
    let ticket = "a".repeat(64);
    {
        let mut state = gate.lock().unwrap();
        state.connection = Some(Connection {
            id: 1,
            writer: Arc::new(Mutex::new(writer)),
            ack: 0,
        });
        state
            .tickets
            .insert(ticket.clone(), "com.example.app".to_owned());
        state.permits.insert(
            token.clone(),
            Permit {
                package: "com.example.app".to_owned(),
                token,
                caller: "manager",
                ticket: Some(ticket.clone()),
            },
        );
    }
    assert!(gate.ticket_live(&ticket));
    gate.cancel(&ticket).unwrap();
    assert!(!gate.ticket_live(&ticket));
    assert!(gate.lock().unwrap().permits.is_empty());
    drop(reader);
}

#[test]
fn a_disconnected_system_cannot_issue_an_explicit_launch_permit() {
    let root = tempfile::tempdir().unwrap();
    stored_package(root.path());
    let gate = LaunchGate::new(root.path());
    assert!(gate.permit_explicit("com.example.app").is_err());
    assert!(gate.lock().unwrap().permits.is_empty());
}

#[test]
fn a_permit_wait_does_not_accept_another_system_connection() {
    let root = tempfile::tempdir().unwrap();
    let gate = LaunchGate::new(root.path());
    let (_reader, writer) = UnixStream::pair().unwrap();
    gate.lock().unwrap().connection = Some(Connection {
        id: 2,
        writer: Arc::new(Mutex::new(writer)),
        ack: 9,
    });
    assert!(gate.await_ack(1, 9).is_err());
}

#[test]
fn control_frames_reject_paths_extra_fields_and_unterminated_input() {
    for token in ["../package", "", &"A".repeat(64), &"b".repeat(65)] {
        assert!(validate_token(token).is_err());
    }
    assert!(validate_token(&"a".repeat(64)).is_ok());
    assert!(serde_json::from_str::<Control>(r#"{"op":"status","command":"mount"}"#).is_err());
    let mut oversized = io::Cursor::new(vec![b'x'; FRAME_LIMIT as usize + 1]);
    assert!(read_subscription_frame(&mut oversized).is_err());
    let mut partial = io::Cursor::new(b"{\"op\":\"status\"}");
    assert!(read_subscription_frame(&mut partial).is_err());
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn pid_reuse_discards_an_old_isolated_process_record() {
    let root = tempfile::tempdir().unwrap();
    let actual = ProcessIdentity::read(std::process::id()).unwrap();
    let record = ProcessRecord {
        pid: actual.pid,
        uid: actual.uid,
        started: actual.started + 1,
        package: "com.example.app".to_owned(),
        package_uid: 10123,
        slot: "base".to_owned(),
        namespace: actual.namespace,
    };
    save_process_record(root.path(), &record).unwrap();
    assert!(tracked_processes(root.path(), 10123).unwrap().is_empty());
    assert!(
        !root
            .path()
            .join("processes")
            .join(format!("{}.json", record.pid))
            .exists()
    );
}

#[test]
fn native_requests_have_no_client_supplied_mount_sources() {
    let request = r#"{"op":"specialize","pid":42,"uid":10123,"started":123,"package":"com.example.app","source":"/data/other"}"#;
    assert!(serde_json::from_str::<Control>(request).is_err());
}
