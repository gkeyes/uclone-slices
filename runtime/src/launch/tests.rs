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
    let first = load_policies(root.path(), &BTreeMap::new()).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].uid, 10123);
    assert_eq!(first[0].default.as_deref(), Some("base"));
    aggregate.set_default_slot(None).unwrap();
    FilePackageStore::open(root.path())
        .unwrap()
        .save(&aggregate)
        .unwrap();
    assert_eq!(
        load_policies(root.path(), &BTreeMap::new()).unwrap()[0].default,
        None
    );
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
                slot: "base".to_owned(),
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
    assert!(gate.permit_explicit("com.example.app", "base").is_err());
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
        verified: true,
    };
    save_process_record(root.path(), &record).unwrap();
    assert!(tracked_processes(root.path(), 10123).unwrap().is_empty());
    assert!(
        !root
            .path()
            .join("processes")
            .join(format!("{}-{}.json", record.pid, record.started))
            .exists()
    );
}

#[test]
fn native_requests_have_no_client_supplied_mount_sources() {
    let request = r#"{"op":"specialize","pid":42,"uid":10123,"started":123,"package":"com.example.app","source":"/data/other"}"#;
    assert!(serde_json::from_str::<Control>(request).is_err());
}

#[test]
fn shared_system_snapshot_is_produced_by_the_root_encoder() {
    let token = "a".repeat(64);
    let mut state = State {
        revision: 7,
        policies_valid: true,
        ..State::default()
    };
    state.policies.push(Policy {
        package: "com.example.app".to_owned(),
        uid: 10123,
        active: "slot-1".to_owned(),
        default: Some("base".to_owned()),
        ready: true,
    });
    state.permits.insert(
        token.clone(),
        Permit {
            package: "com.example.app".to_owned(),
            slot: "slot-1".to_owned(),
            token,
            caller: "manager",
            ticket: Some("b".repeat(64)),
        },
    );
    state.processes.insert(
        42,
        ProcessRecord {
            pid: 42,
            uid: 10123,
            started: 123,
            package: "com.example.app".to_owned(),
            package_uid: 10123,
            slot: "slot-1".to_owned(),
            namespace: "mnt:[7]".to_owned(),
            verified: true,
        },
    );
    let expected: Value = serde_json::from_str(include_str!(
        "../../../protocol/gate-fixtures/system-state.json"
    ))
    .unwrap();
    assert_eq!(state_frame(&state), expected);
}

#[test]
fn shared_prepared_response_and_request_use_the_live_private_contract() {
    let expected: Value = serde_json::from_str(include_str!(
        "../../../protocol/gate-fixtures/prepare.response.json"
    ))
    .unwrap();
    assert_eq!(
        prepared_response("com.example.app", &"a".repeat(64)),
        expected
    );
    let request: Control = serde_json::from_str(include_str!(
        "../../../protocol/gate-fixtures/prepare.request.json"
    ))
    .unwrap();
    assert!(matches!(request, Control::Prepare { ticket } if validate_token(&ticket).is_ok()));
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[test]
fn pending_isolated_association_is_kept_in_stop_confirmation() {
    let root = tempfile::tempdir().unwrap();
    let actual = ProcessIdentity::read(std::process::id()).unwrap();
    let mut record = ProcessRecord {
        pid: actual.pid,
        uid: actual.uid,
        started: actual.started,
        package: "com.example.app".to_owned(),
        package_uid: 10123,
        slot: "base".to_owned(),
        namespace: actual.namespace,
        verified: false,
    };
    save_process_record(root.path(), &record).unwrap();
    assert_eq!(
        tracked_processes(root.path(), 10123).unwrap(),
        vec![PathBuf::from(format!("/proc/{}", record.pid))]
    );
    record.verified = true;
    save_process_record(root.path(), &record).unwrap();
    assert_eq!(tracked_processes(root.path(), 10123).unwrap().len(), 1);
}

#[test]
fn process_registry_survives_daemon_restart_but_not_device_reboot() {
    let root = tempfile::tempdir().unwrap();
    reset_process_registry(root.path(), "boot-a").unwrap();
    let directory = root.path().join("processes");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("record"), "old").unwrap();
    reset_process_registry(root.path(), "boot-a").unwrap();
    assert!(directory.join("record").exists());
    reset_process_registry(root.path(), "boot-b").unwrap();
    assert!(!directory.exists());
}

#[test]
fn one_unreadable_aggregate_does_not_invalidate_other_policies() {
    let root = tempfile::tempdir().unwrap();
    stored_package(root.path());
    let broken = root.path().join("packages").join("com.example.broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("aggregate.json"), b"{not-json").unwrap();

    let policies = load_policies(root.path(), &BTreeMap::new()).unwrap();
    assert_eq!(policies.len(), 1);
    assert_eq!(policies[0].package, "com.example.app");

    let known = BTreeMap::from([("com.example.broken".to_owned(), 10200)]);
    let policies = load_policies(root.path(), &known).unwrap();
    let broken = policies
        .iter()
        .find(|policy| policy.package == "com.example.broken")
        .unwrap();
    assert_eq!(broken.uid, 10200);
    assert!(!broken.ready);
    assert!(
        policies
            .iter()
            .any(|policy| policy.package == "com.example.app")
    );

    let gate = LaunchGate::new(root.path());
    gate.refresh().unwrap();
    assert!(gate.lock().unwrap().policies_valid);
}

#[test]
fn a_begin_for_an_unready_package_keeps_the_system_subscription() {
    let root = tempfile::tempdir().unwrap();
    let gate = LaunchGate::new(root.path());
    let (daemon, mut system) = UnixStream::pair().unwrap();
    let subscriber = Arc::clone(&gate);
    let handle = thread::spawn(move || subscriber.subscribe(BufReader::new(daemon)));
    let begin = json!({"type": "begin", "ticket": "a".repeat(64), "package": "com.example.app"});
    system.write_all(format!("{begin}\n").as_bytes()).unwrap();
    // An Ack beyond the published revision ends the subscription with its own error,
    // which proves the refused Begin did not end it first.
    system
        .write_all(b"{\"type\":\"ack\",\"revision\":999999}\n")
        .unwrap();
    let error = handle.join().unwrap().unwrap_err();
    assert_eq!(error.to_string(), "invalid revision");
    assert!(gate.lock().unwrap().tickets.is_empty());
}

#[test]
fn blocking_without_a_system_subscription_does_not_fail_the_transaction() {
    let root = tempfile::tempdir().unwrap();
    stored_package(root.path());
    let gate = LaunchGate::new(root.path());
    gate.block_new_processes("com.example.app").unwrap();
    assert!(gate.lock().unwrap().blocked.contains("com.example.app"));
    assert!(!gate.await_system(Duration::from_millis(50)).unwrap());
}

#[test]
fn a_stalled_process_view_worker_is_killed_at_its_deadline() {
    let mut command = Command::new("sleep");
    command.arg("30");
    let started = Instant::now();
    let error = run_worker(command, Duration::from_millis(100)).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(started.elapsed() < Duration::from_secs(5));
    let mut command = Command::new("sh");
    command.args(["-c", "echo stage >&2; exit 3"]);
    let output = run_worker(command, Duration::from_secs(5)).unwrap();
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(output.stderr, b"stage\n");
}

#[test]
fn previous_discovery_is_read_only_from_an_owned_regular_file() {
    let root = tempfile::tempdir().unwrap();
    let owner = fs::metadata(root.path()).unwrap().uid();
    let manifest = br#"{"packages":[{"package":"com.example.app","uid":10123}]}"#;
    fs::write(root.path().join("elsewhere.json"), manifest).unwrap();

    std::os::unix::fs::symlink(
        root.path().join("elsewhere.json"),
        root.path().join("enrollment.json"),
    )
    .unwrap();
    assert!(read_enrollment_uids(root.path(), owner).is_empty());

    fs::remove_file(root.path().join("enrollment.json")).unwrap();
    fs::write(root.path().join("enrollment.json"), manifest).unwrap();
    assert!(read_enrollment_uids(root.path(), owner.wrapping_add(1)).is_empty());
    assert_eq!(
        read_enrollment_uids(root.path(), owner),
        BTreeMap::from([("com.example.app".to_owned(), 10123)])
    );
}
