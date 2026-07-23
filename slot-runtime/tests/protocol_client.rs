#![allow(
    missing_docs,
    clippy::unwrap_used,
    reason = "local Unix socket fixtures fail the invoking test immediately"
)]

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;

use uclone_slot_runtime::protocol::{
    Command, ProbeReport, ProtocolError, Request, RequestId, Response, ResponsePayload,
    ResponseStatus, UnixClient, decode_request, encode_response,
};

fn id(value: &str) -> RequestId {
    RequestId::new(value).unwrap()
}

fn read_request_frame(stream: &mut UnixStream) -> Vec<u8> {
    let mut received = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).unwrap();
        received.push(byte[0]);
        if byte[0] == b'\n' {
            return received;
        }
    }
}

#[test]
fn unix_client_writes_one_frame_and_matches_response_id() {
    let (client_stream, mut server_stream) = UnixStream::pair().unwrap();
    let request = Request::new(id("r1"), Command::Probe).unwrap();
    let response_id = request.request_id().clone();
    let server = thread::spawn(move || {
        let received = read_request_frame(&mut server_stream);
        assert_eq!(
            decode_request(&received).unwrap().request_id(),
            &response_id
        );
        let response = Response::ok(
            response_id,
            ResponsePayload::ProbeReport(ProbeReport::new(true, true, true, false)),
        )
        .unwrap();
        server_stream
            .write_all(&encode_response(&response).unwrap())
            .unwrap();
    });

    let mut client = UnixClient::from_stream(client_stream);
    let response = client.request(&request).unwrap();
    assert_eq!(response.status(), ResponseStatus::Ok);
    server.join().unwrap();
}

#[test]
fn unix_client_rejects_mismatched_response_request_id() {
    let (client_stream, mut server_stream) = UnixStream::pair().unwrap();
    let request = Request::new(id("r1"), Command::Probe).unwrap();
    let server = thread::spawn(move || {
        let received = read_request_frame(&mut server_stream);
        assert_eq!(decode_request(&received).unwrap().request_id(), &id("r1"));
        let response = Response::ok(
            id("other"),
            ResponsePayload::ProbeReport(ProbeReport::new(true, true, true, false)),
        )
        .unwrap();
        server_stream
            .write_all(&encode_response(&response).unwrap())
            .unwrap();
    });

    let mut client = UnixClient::from_stream(client_stream);
    assert!(matches!(
        client.request(&request),
        Err(ProtocolError::RequestIdMismatch { .. })
    ));
    server.join().unwrap();
}

#[test]
fn unix_client_read_is_bounded_by_configured_timeout() {
    let (client_stream, _server_stream) = UnixStream::pair().unwrap();
    let request = Request::new(id("r-timeout"), Command::Probe).unwrap();
    let mut client = UnixClient::from_stream(client_stream);
    client.set_timeout(Duration::from_millis(20)).unwrap();
    let error = client.request(&request).unwrap_err();
    assert!(matches!(
        error,
        ProtocolError::Io(ref io_error)
            if matches!(io_error.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock)
    ));
}
