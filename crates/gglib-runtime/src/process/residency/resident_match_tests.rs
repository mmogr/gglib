//! A resident launched with another projector, or another context, is
//! recycled; one launched the same way is kept.

use std::io::{Read, Write};
use std::path::Path;

use super::launched_differently;
use crate::process::admission::{PRIMARY_SLOT, Resident};
use crate::process::residency::ResidentSet;
use crate::process::residency::hold_tests::{core, granted, resident, set_with_resident};

const PROJECTOR: &str = "/models/mmproj-F16.gguf";

fn linked(port: u16) -> Resident {
    Resident {
        projector: Some(PROJECTOR.into()),
        ..resident(port)
    }
}

#[test]
fn the_same_context_and_projector_is_the_same_launch() {
    assert!(!launched_differently(&resident(8080), (4096, None)));
    assert!(!launched_differently(
        &linked(8080),
        (4096, Some(Path::new(PROJECTOR)))
    ));
}

#[test]
fn another_context_is_a_different_launch() {
    assert!(launched_differently(&resident(8080), (8192, None)));
    assert!(launched_differently(
        &linked(8080),
        (8192, Some(Path::new(PROJECTOR)))
    ));
}

#[test]
fn another_projector_is_a_different_launch() {
    let request = Some(Path::new(PROJECTOR));
    assert!(
        launched_differently(&resident(8080), (4096, request)),
        "none, then one"
    );
    assert!(
        launched_differently(&linked(8080), (4096, None)),
        "one, then none"
    );
    let other = Some(Path::new("/models/other-mmproj.gguf"));
    assert!(
        launched_differently(&linked(8080), (4096, other)),
        "one, then another"
    );
}

/// A port whose `/health` answers 200 to every request, as a llama-server
/// that is up does. With the health check passing, only the launch
/// comparison can recycle the resident in the tests below.
pub(super) fn healthy_port() -> u16 {
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = server.local_addr().unwrap().port();
    answer_health(server);
    port
}

/// Answers every request `server` accepts with llama-server's healthy reply,
/// from a thread of its own.
pub(super) fn answer_health(server: std::net::TcpListener) {
    std::thread::spawn(move || {
        for mut stream in server.incoming().flatten() {
            // The whole request head is read before answering: a peer that
            // answers and closes with bytes unread resets the connection.
            let mut head = Vec::new();
            let mut byte = [0_u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
                head.push(byte[0]);
            }
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}",
            );
        }
    });
}

/// A set whose primary is `resident`, with a request for it already granted.
fn granted_set(resident: Resident) -> ResidentSet {
    let set = set_with_resident(resident.port);
    drop(set.queue().evict(PRIMARY_SLOT));
    drop(set.queue().install(PRIMARY_SLOT, resident));
    granted(&set);
    set
}

/// The control for the two tests after it: launched as asked and healthy, the
/// resident serves.
#[tokio::test]
async fn a_healthy_resident_launched_with_the_same_projector_serves() {
    let set = granted_set(linked(healthy_port()));

    let request = (4096, Some(Path::new(PROJECTOR)));
    let served = set.serve(PRIMARY_SLOT, request, &core()).await.unwrap();

    assert!(served.is_some(), "kept and served");
    assert!(set.queue().slot(PRIMARY_SLOT).is_some());
}

/// A model linked to a projector since it was launched is stopped and
/// forgotten, so the next pass launches it with `--mmproj`.
#[tokio::test]
async fn a_healthy_resident_launched_without_the_projector_now_linked_is_recycled() {
    let set = granted_set(resident(healthy_port()));

    let request = (4096, Some(Path::new(PROJECTOR)));
    let served = set.serve(PRIMARY_SLOT, request, &core()).await.unwrap();

    assert!(served.is_none());
    assert!(set.queue().slot(PRIMARY_SLOT).is_none(), "recycled");
}

/// The other direction: unlinked since launch.
#[tokio::test]
async fn a_healthy_resident_launched_with_a_projector_since_unlinked_is_recycled() {
    let set = granted_set(linked(healthy_port()));

    let served = set
        .serve(PRIMARY_SLOT, (4096, None), &core())
        .await
        .unwrap();

    assert!(served.is_none());
    assert!(set.queue().slot(PRIMARY_SLOT).is_none(), "recycled");
}
