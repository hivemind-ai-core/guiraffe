//! The local socket: the calls of the API, the answers and the limits on
//! what a client may ask for.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use guiraffe_test::transport::{CallSocket, MAX_CLIENTS};

fn socket_path(name: &str) -> std::path::PathBuf {
    let dir = tempfile::tempdir().expect("a temporary directory");
    // The directory lives as long as the socket, so the path stays valid.
    let path = dir.path().join(name);
    std::mem::forget(dir);
    path
}

#[test]
fn a_call_comes_in_and_the_answer_goes_back() {
    let path = socket_path("inspect.sock");
    let socket = CallSocket::start(&path).expect("the socket starts");
    let mut client = UnixStream::connect(&path).expect("the client connects");
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("a timeout");
    writeln!(client, "{{\"method\":\"style\"}}").expect("the call");
    client.flush().expect("the call is sent");

    let (id, call) = socket.next_call().expect("the call comes in");
    assert!(call.contains("style"), "{call}");

    socket.answer(id, "{\"ok\":true}");
    let mut reader = BufReader::new(client.try_clone().expect("a copy"));
    let mut answer = String::new();
    reader.read_line(&mut answer).expect("the answer");
    assert!(answer.contains("\"ok\":true"), "{answer}");
}

#[test]
fn a_request_that_is_too_large_is_refused() {
    let path = socket_path("big.sock");
    let _socket = CallSocket::start(&path).expect("the socket starts");
    let mut client = UnixStream::connect(&path).expect("the client connects");
    client
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .expect("a timeout");

    // A request larger than a mebibyte is refused by the socket.
    let huge = "x".repeat(2 * 1024 * 1024);
    let _ = writeln!(client, "{huge}");
    let mut reader = BufReader::new(client.try_clone().expect("a copy"));
    let mut answer = String::new();
    let read = reader.read_line(&mut answer);
    assert!(read.is_err() || answer.is_empty(), "{answer}");
}

#[test]
fn the_socket_serves_at_most_eight_clients() {
    let path = socket_path("many.sock");
    let _socket = CallSocket::start(&path).expect("the socket starts");
    let mut clients = Vec::new();
    for _ in 0..MAX_CLIENTS {
        clients.push(UnixStream::connect(&path).expect("the client connects"));
    }
    // The next client is closed again right away.
    let extra = UnixStream::connect(&path).expect("the client connects");
    extra
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .expect("a timeout");
    let mut reader = BufReader::new(extra.try_clone().expect("a copy"));
    let mut answer = String::new();
    let read = reader.read_line(&mut answer);
    assert!(read.is_ok(), "the connection is refused, not broken");
    assert_eq!(answer.len(), 0, "the refused client reads nothing");
}
