#![allow(clippy::unwrap_used)]

use super::*;
use std::net::Shutdown;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const PRODUCTION_TIMEOUTS: Timeouts = Timeouts {
    read: REQUEST_READ_TIMEOUT,
    write: RESPONSE_WRITE_TIMEOUT,
};

fn read_timeout(read: Duration) -> Timeouts {
    Timeouts {
        read,
        ..PRODUCTION_TIMEOUTS
    }
}

fn permit() -> ConnectionPermit {
    ConnectionLimit::new(MAX_CONCURRENT_CONNECTIONS).acquire()
}

#[test]
fn one_connection_carries_one_request() {
    let (mut client, server) = UnixStream::pair().unwrap();
    client.write_all(b"{\"op\":\"future\"}\n").unwrap();
    client.shutdown(Shutdown::Write).unwrap();

    let mut received = String::new();
    serve_one(server, PRODUCTION_TIMEOUTS, |line| {
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
    let stalled = spawn_connection(stalled_server, Arc::clone(&handler), permit());
    let (mut ready_client, ready_server) = UnixStream::pair().unwrap();
    let ready = spawn_connection(ready_server, handler, permit());

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
    let worker = spawn_connection(server, handler, permit());
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
    let first = spawn_connection(first_server, Arc::clone(&handler), permit());
    let second = spawn_connection(second_server, handler, permit());
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

#[test]
fn a_client_that_never_finishes_its_line_is_dropped_after_the_read_timeout() {
    let (mut client, server) = UnixStream::pair().unwrap();
    client.write_all(b"{\"op\":").unwrap();
    let mut called = false;

    let error = serve_one(server, read_timeout(Duration::from_millis(100)), |_line| {
        called = true;
        String::new()
    })
    .unwrap_err();

    assert!(!called);
    let error = error.downcast::<std::io::Error>().unwrap();
    assert!(matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    ));
}

#[test]
fn the_read_timeout_bounds_the_whole_line_not_each_read() {
    let (mut client, server) = UnixStream::pair().unwrap();
    let trickle = thread::spawn(move || {
        // Every byte arrives well inside the timeout, but the line never ends in time.
        for _ in 0..50 {
            if client.write_all(b" ").is_err() {
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }
    });

    let started = Instant::now();
    let result = serve_one(server, read_timeout(Duration::from_millis(200)), |_line| {
        String::new()
    });

    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_millis(1_500));
    trickle.join().unwrap();
}

#[test]
fn a_prompt_request_is_served_well_within_the_production_timeout() {
    let (mut client, server) = UnixStream::pair().unwrap();
    let worker = spawn_connection(server, Arc::new(|line: &str| format!("{line}\n")), permit());
    thread::sleep(Duration::from_millis(50));
    client.write_all(b"{\"op\":\"probe\"}\n").unwrap();
    client.shutdown(Shutdown::Write).unwrap();

    let mut response = String::new();
    std::io::Read::read_to_string(&mut client, &mut response).unwrap();
    worker.join().unwrap();
    assert_eq!(response, "{\"op\":\"probe\"}\n");
}

#[test]
fn connections_over_the_limit_wait_for_a_free_slot_instead_of_failing() {
    let limit = ConnectionLimit::new(2);
    let first = limit.acquire();
    let _second = limit.acquire();
    let (acquired_sender, acquired) = std::sync::mpsc::channel();
    let waiter = thread::spawn({
        let limit = Arc::clone(&limit);
        move || {
            let third = limit.acquire();
            acquired_sender.send(()).unwrap();
            drop(third);
        }
    });

    assert!(acquired.recv_timeout(Duration::from_millis(100)).is_err());
    drop(first);
    acquired.recv_timeout(Duration::from_secs(5)).unwrap();
    waiter.join().unwrap();
}

#[test]
fn a_client_that_stops_reading_its_response_is_dropped_after_the_write_timeout() {
    let (mut client, server) = UnixStream::pair().unwrap();
    client.write_all(b"{\"op\":\"list_packages\"}\n").unwrap();
    // Far more than the socket buffer holds; the client never reads any of it.
    let large = "x".repeat(16 * 1024 * 1024);

    let started = Instant::now();
    let result = serve_one(
        server,
        Timeouts {
            read: REQUEST_READ_TIMEOUT,
            write: Duration::from_millis(200),
        },
        |_line| large,
    );

    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(client);
}

#[test]
fn a_large_response_read_promptly_is_delivered_in_full() {
    let (mut client, server) = UnixStream::pair().unwrap();
    let expected = "y".repeat(4 * 1024 * 1024);
    let response = expected.clone();
    let worker =
        thread::spawn(move || serve_one(server, PRODUCTION_TIMEOUTS, |_line| response).is_ok());
    client.write_all(b"{\"op\":\"list_packages\"}\n").unwrap();
    client.shutdown(Shutdown::Write).unwrap();

    let mut received = String::new();
    std::io::Read::read_to_string(&mut client, &mut received).unwrap();
    assert!(worker.join().unwrap());
    assert_eq!(received.len(), expected.len());
}
