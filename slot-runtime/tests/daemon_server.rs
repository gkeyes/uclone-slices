#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    missing_docs,
    reason = "test fixtures should fail the individual test immediately"
)]

use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::UnixListener;
#[cfg(target_os = "macos")]
use std::process::Command as ProcessCommand;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tempfile::TempDir;
use uclone_slot_runtime::daemon::{DaemonError, DaemonServer, MutationGuard, RequestHandler};
#[cfg(target_os = "macos")]
use uclone_slot_runtime::daemon::{RuntimeLock, RuntimeLockError};
use uclone_slot_runtime::protocol::{
    Command, ErrorCode, ProbeReport, Request, RequestId, Response, ResponsePayload, ResponseStatus,
    UnixClient, decode_response, encode_request,
};

fn id(value: &str) -> RequestId {
    RequestId::new(value).unwrap()
}

fn probe() -> Request {
    Request::new(id("daemon-test"), Command::Probe).unwrap()
}

fn probe_response(request: &Request) -> Response {
    Response::ok(
        request.request_id().clone(),
        ResponsePayload::ProbeReport(ProbeReport::new(true, true, true, false)),
    )
    .unwrap()
}

#[derive(Debug, Default)]
struct ProbeHandler {
    calls: Arc<Mutex<usize>>,
}

impl RequestHandler for ProbeHandler {
    fn handle(&mut self, request: &Request) -> Response {
        *self.calls.lock().unwrap() += 1;
        probe_response(request)
    }
}

fn socket_path() -> (TempDir, std::path::PathBuf) {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("run").join("ucloned.sock");
    (temp, path)
}

#[test]
fn bind_at_creates_private_parent_and_socket_and_serves_one_exchange() {
    let (_temp, path) = socket_path();
    let mut server = DaemonServer::bind_at(&path, ProbeHandler::default()).unwrap();
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let join = thread::spawn(move || server.run_once());
    let mut client = UnixClient::connect(&path).unwrap();
    let response = client.request(&probe()).unwrap();
    assert_eq!(response.status(), ResponseStatus::Ok);
    assert_eq!(response.request_id().as_str(), "daemon-test");
    let mut stream = client.into_stream();
    let mut byte = [0_u8; 1];
    assert_eq!(stream.read(&mut byte).unwrap(), 0);
    join.join().unwrap().unwrap();
}

#[test]
fn parent_symlink_and_non_socket_stale_path_are_rejected() {
    let temp = TempDir::new().unwrap();
    let real_parent = temp.path().join("real");
    fs::create_dir(&real_parent).unwrap();
    let linked_parent = temp.path().join("run");
    symlink(&real_parent, &linked_parent).unwrap();
    let linked_socket = linked_parent.join("ucloned.sock");
    assert!(matches!(
        DaemonServer::bind_at(linked_socket, ProbeHandler::default()),
        Err(DaemonError::SymlinkPath(_))
    ));

    let regular_path = temp.path().join("regular.sock");
    fs::write(&regular_path, b"stale").unwrap();
    assert!(matches!(
        DaemonServer::bind_at(regular_path, ProbeHandler::default()),
        Err(DaemonError::NonSocketPath(_))
    ));
}

#[test]
fn stale_unlistened_socket_is_replaced() {
    let (_temp, path) = socket_path();
    fs::create_dir(path.parent().unwrap()).unwrap();
    let listener = UnixListener::bind(&path).unwrap();
    drop(listener);
    let server = DaemonServer::bind_at(&path, ProbeHandler::default()).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    drop(server);
}

#[test]
fn malformed_request_with_safe_id_gets_invalid_request_response() {
    let (_temp, path) = socket_path();
    let mut server = DaemonServer::bind_at(&path, ProbeHandler::default()).unwrap();
    let join = thread::spawn(move || server.run_once());
    let mut stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
    stream
        .write_all(
            br#"{"schema_version":2,"request_id":"safe-id","command":"nope"}
"#,
        )
        .unwrap();
    let mut response_frame = Vec::new();
    stream.read_to_end(&mut response_frame).unwrap();
    let response = decode_response(&response_frame).unwrap();
    assert_eq!(response.request_id().as_str(), "safe-id");
    assert_eq!(response.error_code(), Some(ErrorCode::InvalidRequest));
    join.join().unwrap().unwrap();
}

#[test]
fn build_mismatch_is_rejected_before_dispatch() {
    let (_temp, path) = socket_path();
    let handler = ProbeHandler::default();
    let calls = Arc::clone(&handler.calls);
    let mut server = DaemonServer::bind_at(&path, handler).unwrap();
    let join = thread::spawn(move || server.run_once());
    let mut stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
    stream
        .write_all(
            br#"{"schema_version":2,"request_id":"pair-id","command":"status_package","build_id":"other","package":"com.example.app"}
"#,
        )
        .unwrap();
    let mut response_frame = Vec::new();
    stream.read_to_end(&mut response_frame).unwrap();
    let response = decode_response(&response_frame).unwrap();
    assert_eq!(response.error_code(), Some(ErrorCode::RuntimePairMismatch));
    assert_eq!(*calls.lock().unwrap(), 0);
    join.join().unwrap().unwrap();
}

#[test]
fn malformed_request_without_safe_id_is_closed_without_a_fabricated_id() {
    let (_temp, path) = socket_path();
    let mut server = DaemonServer::bind_at(&path, ProbeHandler::default()).unwrap();
    let join = thread::spawn(move || server.run_once());
    let mut stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
    stream.write_all(b"not-json\n").unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut response_frame = Vec::new();
    stream.read_to_end(&mut response_frame).unwrap();
    assert!(response_frame.is_empty());
    join.join().unwrap().unwrap();
}

#[test]
fn oversized_frame_with_safe_id_gets_invalid_request_response() {
    let (_temp, path) = socket_path();
    let mut server = DaemonServer::bind_at(&path, ProbeHandler::default()).unwrap();
    let join = thread::spawn(move || server.run_once());
    let mut stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
    let mut frame =
        br#"{"schema_version":2,"request_id":"big-id","command":"probe","padding":""#.to_vec();
    frame.extend(std::iter::repeat_n(b'x', 16 * 1024));
    frame.extend_from_slice(b"\"}\n");
    stream.write_all(&frame).unwrap();
    let mut response_frame = Vec::new();
    stream.read_to_end(&mut response_frame).unwrap();
    let response = decode_response(&response_frame).unwrap();
    assert_eq!(response.request_id().as_str(), "big-id");
    assert_eq!(response.error_code(), Some(ErrorCode::InvalidRequest));
    join.join().unwrap().unwrap();
}

#[test]
fn read_timeout_is_applied_to_each_connection() {
    let (_temp, path) = socket_path();
    let server = DaemonServer::bind_at(&path, ProbeHandler::default())
        .unwrap()
        .with_timeout(Duration::from_millis(20))
        .unwrap();
    let join = thread::spawn(move || {
        let mut server = server;
        server.run_once()
    });
    let _stream = std::os::unix::net::UnixStream::connect(&path).unwrap();
    assert!(matches!(
        join.join().unwrap(),
        Err(DaemonError::Io(error))
            if matches!(error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock)
    ));
}

#[test]
fn mutation_guard_is_global_and_reports_busy_without_blocking() {
    let guard = MutationGuard::new();
    let permit = guard.try_lock().unwrap();
    assert!(matches!(guard.try_lock(), Err(ErrorCode::Busy)));
    drop(permit);
    assert!(guard.try_acquire().is_ok());
}

#[cfg(target_os = "macos")]
#[test]
fn host_runtime_lock_never_reclaims_a_foreign_live_owner_without_process_proof() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("runtime.lock");
    fs::create_dir(&path).unwrap();
    let mut child = ProcessCommand::new("/bin/sleep").arg("5").spawn().unwrap();
    fs::write(
        path.join("owner"),
        format!(
            "pid={}\nboot=host-test-boot\nstart_ticks=1\nrole=ucloned\n",
            child.id()
        ),
    )
    .unwrap();
    assert!(matches!(
        RuntimeLock::acquire(&path, "slotctl"),
        Err(RuntimeLockError::Busy)
    ));
    child.kill().unwrap();
    child.wait().unwrap();
}

#[test]
fn request_encoding_fixture_remains_one_frame() {
    let frame = encode_request(&probe()).unwrap();
    assert!(frame.len() <= 16 * 1024);
    assert_eq!(frame.last().copied(), Some(b'\n'));
}
