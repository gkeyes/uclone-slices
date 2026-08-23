#![allow(clippy::unwrap_used)]

use super::*;
use std::net::Shutdown;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[test]
fn one_connection_carries_one_request() {
    let (mut client, server) = UnixStream::pair().unwrap();
    client.write_all(b"{\"op\":\"future\"}\n").unwrap();
    client.shutdown(Shutdown::Write).unwrap();

    let mut received = String::new();
    serve_one(server, |line| {
        received = line.to_owned();
        INVALID_REQUEST_RESPONSE.to_owned()
    })
    .unwrap();

    let mut response = String::new();
    std::io::Read::read_to_string(&mut client, &mut response).unwrap();
    assert_eq!(received, "{\"op\":\"future\"}");
    assert_eq!(response, INVALID_REQUEST_RESPONSE);
}

#[test]
fn stalled_frame_does_not_block_a_complete_request() {
    let calls = Arc::new(Mutex::new(0_u8));
    let calls_for_handler = Arc::clone(&calls);
    let handler: Arc<RequestHandler> = Arc::new(move |_line| {
        if let Ok(mut calls) = calls_for_handler.lock() {
            *calls += 1;
        }
        "{\"ok\":{}}\n".to_owned()
    });
    let (mut stalled_client, stalled_server) = UnixStream::pair().unwrap();
    stalled_client.write_all(b"{").unwrap();
    let stalled = spawn_connection(stalled_server, Arc::clone(&handler));
    let (mut ready_client, ready_server) = UnixStream::pair().unwrap();
    let ready = spawn_connection(ready_server, handler);

    ready_client.write_all(b"{\"op\":\"probe\"}\n").unwrap();
    ready_client.shutdown(Shutdown::Write).unwrap();
    let mut ready_response = String::new();
    std::io::Read::read_to_string(&mut ready_client, &mut ready_response).unwrap();

    assert_eq!(ready_response, "{\"ok\":{}}\n");
    assert_eq!(*calls.lock().unwrap(), 1);
    stalled_client.write_all(b"}\n").unwrap();
    stalled_client.shutdown(Shutdown::Write).unwrap();
    let mut stalled_response = String::new();
    std::io::Read::read_to_string(&mut stalled_client, &mut stalled_response).unwrap();
    ready.join().unwrap();
    stalled.join().unwrap();
    assert_eq!(*calls.lock().unwrap(), 2);
}

#[test]
fn oversized_frame_is_rejected_without_entering_the_handler() {
    let calls = Arc::new(Mutex::new(0_u8));
    let calls_for_handler = Arc::clone(&calls);
    let handler: Arc<RequestHandler> = Arc::new(move |_line| {
        if let Ok(mut calls) = calls_for_handler.lock() {
            *calls += 1;
        }
        "unexpected\n".to_owned()
    });
    let (mut client, server) = UnixStream::pair().unwrap();
    let worker = spawn_connection(server, handler);
    client.write_all(&vec![b'a'; MAX_FRAME_BYTES + 1]).unwrap();
    client.write_all(b"\n").unwrap();
    client.shutdown(Shutdown::Write).unwrap();

    let mut response = String::new();
    std::io::Read::read_to_string(&mut client, &mut response).unwrap();

    worker.join().unwrap();
    assert_eq!(response, INVALID_REQUEST_RESPONSE);
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[test]
fn complete_transactions_remain_strictly_serial() {
    let transaction_lock = Arc::new(Mutex::new(()));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let handler: Arc<RequestHandler> = Arc::new({
        let transaction_lock = Arc::clone(&transaction_lock);
        let active = Arc::clone(&active);
        let maximum = Arc::clone(&maximum);
        move |_line| {
            serialize_transaction(&transaction_lock, || {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(20));
                active.fetch_sub(1, Ordering::SeqCst);
                "{\"ok\":{}}\n".to_owned()
            })
        }
    });
    let (mut first_client, first_server) = UnixStream::pair().unwrap();
    let (mut second_client, second_server) = UnixStream::pair().unwrap();
    let first = spawn_connection(first_server, Arc::clone(&handler));
    let second = spawn_connection(second_server, handler);
    first_client.write_all(b"{\"op\":\"probe\"}\n").unwrap();
    second_client.write_all(b"{\"op\":\"probe\"}\n").unwrap();
    first_client.shutdown(Shutdown::Write).unwrap();
    second_client.shutdown(Shutdown::Write).unwrap();

    let mut first_response = String::new();
    let mut second_response = String::new();
    std::io::Read::read_to_string(&mut first_client, &mut first_response).unwrap();
    std::io::Read::read_to_string(&mut second_client, &mut second_response).unwrap();
    first.join().unwrap();
    second.join().unwrap();

    assert_eq!(first_response, "{\"ok\":{}}\n");
    assert_eq!(second_response, "{\"ok\":{}}\n");
    assert_eq!(maximum.load(Ordering::SeqCst), 1);
}
