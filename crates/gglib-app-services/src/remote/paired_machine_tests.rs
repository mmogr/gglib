//! Tests for the name the paired machine is shown by: what a connect reads
//! and keeps, what a re-pair keeps and drops, and that a far machine that
//! will not say never costs the join.
//!
//! The read is driven through [`learn_name_through`] with a far proxy on a
//! loopback port, because the live connection `RemoteOps::learn_name` reads
//! through wants an iroh endpoint and a peer that answers. The join that
//! reads it over a real pipe is `join_key_pipe_tests.rs`'s.

use std::time::{Duration, Instant};

use gglib_core::RemotePairing;
use gglib_core::domain::UNNAMED_PAIRED;
use gglib_core::services::AppCore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::super::stored_pairing::store_redeemed;
use super::{FarCredentials, far_credentials, keep_name, learn_name_through};
use crate::error::GuiError;
use crate::remote::far_proxy::FarProxy;
use crate::test_support::test_core;
use crate::test_support_remote::{
    FINGERPRINT_A, KEY_A, KEY_B, TICKET_A, TICKET_A_MOVED, TICKET_B, paired_with, ticket,
};

/// The stored pairing, which each test here expects to exist.
async fn stored(core: &AppCore) -> RemotePairing {
    core.settings()
        .get()
        .await
        .expect("settings load")
        .remote_pairing
        .expect("a pairing is stored")
}

/// Machine A's pairing stored, under `name` when one is given.
async fn paired_with_a(name: Option<&str>) -> std::sync::Arc<AppCore> {
    let core = test_core().await;
    core.settings()
        .update(paired_with(TICKET_A, KEY_A))
        .await
        .expect("machine A's pairing is stored");
    if let Some(name) = name {
        keep_name(&core, FINGERPRINT_A, name)
            .await
            .expect("the name is kept");
    }
    core
}

/// A far proxy for machine A that answers every request with `response`, a
/// whole HTTP/1.1 answer, or with nothing at all when it is `None`.
async fn far_answering(response: Option<&'static str>) -> FarProxy {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut byte = [0u8; 1];
                while !head.ends_with(b"\r\n\r\n") {
                    if socket.read_exact(&mut byte).await.is_err() {
                        return;
                    }
                    head.push(byte[0]);
                }
                match response {
                    Some(response) => {
                        let _ = socket.write_all(response.as_bytes()).await;
                    }
                    None => tokio::time::sleep(Duration::from_secs(30)).await,
                }
            });
        }
    });
    let credentials = FarCredentials {
        key: KEY_A.to_owned(),
        fingerprint: FINGERPRINT_A.to_owned(),
        name: None,
    };
    FarProxy::new(&format!("http://127.0.0.1:{port}/v1"), &credentials).unwrap()
}

/// A model list naming its machine `Desk.local`, with no models in it.
const LISTED: &str = "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 55\r\n\
                      connection: close\r\n\r\n\
                      {\"object\":\"list\",\"data\":[],\"machine_name\":\"Desk.local\"}";

/// A far proxy that fails the read.
const FAILED: &str = "HTTP/1.1 500 Internal Server Error\r\ncontent-length: 0\r\n\
                      connection: close\r\n\r\n";

/// The name a connected machine's list gives is kept on its pairing as
/// `machine_name` reads it, and handed back for `join` to answer with.
#[tokio::test]
async fn the_name_the_far_machine_gives_is_kept_on_its_pairing() {
    let core = paired_with_a(None).await;

    let learned =
        learn_name_through(&core, Ok(far_answering(Some(LISTED)).await), FINGERPRINT_A).await;

    assert_eq!(learned.as_deref(), Some("Desk"));
    assert_eq!(stored(&core).await.name.as_deref(), Some("Desk"));
}

/// A far machine that refuses the read, does not answer it, or cannot be
/// reached at all costs nothing: the name it had stays, and the answer is
/// that name, never an error a join could fail on. The silent one is cut at
/// the model list's three seconds.
#[tokio::test]
async fn a_far_machine_that_will_not_say_keeps_the_name_it_had() {
    let core = paired_with_a(Some("desk")).await;

    let refused = learn_name_through(&core, Ok(far_answering(Some(FAILED)).await), FINGERPRINT_A);
    assert_eq!(refused.await.as_deref(), Some("desk"));

    let started = Instant::now();
    let silent = learn_name_through(&core, Ok(far_answering(None).await), FINGERPRINT_A).await;
    assert_eq!(silent.as_deref(), Some("desk"));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the read was not bounded"
    );

    let unreachable = Err(GuiError::Conflict("not connected".to_owned()));
    let gone = learn_name_through(&core, unreachable, FINGERPRINT_A).await;
    assert_eq!(gone.as_deref(), Some("desk"));
}

/// A name is written only onto the pairing of the machine it came from: one
/// paired with another machine meanwhile is not given it.
#[tokio::test]
async fn a_name_is_never_written_onto_another_machines_pairing() {
    let core = test_core().await;
    core.settings()
        .update(paired_with(TICKET_B, KEY_B))
        .await
        .expect("machine B's pairing is stored");

    keep_name(&core, FINGERPRINT_A, "desk")
        .await
        .expect("nothing to write is not a failure");

    assert_eq!(
        stored(&core).await.name,
        None,
        "machine B was given A's name"
    );
}

/// A name `machine_name` does not keep is dropped, not stored, and the one
/// already held stays.
#[tokio::test]
async fn a_name_that_is_not_a_plain_host_label_is_dropped() {
    let core = paired_with_a(Some("desk")).await;

    for raw in ["", "two words", "desk/../etc", "d\u{e9}sk", "\u{1b}[31mred"] {
        keep_name(&core, FINGERPRINT_A, raw)
            .await
            .expect("a dropped name is not a failure");
        assert_eq!(stored(&core).await.name.as_deref(), Some("desk"), "{raw:?}");
    }
}

/// Pairing with the same machine again, at its address or one it moved to,
/// keeps its name; pairing with another drops it, and says which machine
/// was replaced by the name it was shown by.
#[tokio::test]
async fn a_re_pair_keeps_the_name_for_the_same_machine_and_drops_it_for_another() {
    let core = paired_with_a(Some("desk")).await;

    let again = store_redeemed(&core, KEY_A.to_owned(), &ticket(TICKET_A_MOVED), 8181)
        .await
        .expect("machine A is paired again");
    assert_eq!(again, None, "a re-pair with the same machine replaced one");
    assert_eq!(stored(&core).await.name.as_deref(), Some("desk"));

    let other = store_redeemed(&core, KEY_B.to_owned(), &ticket(TICKET_B), 8181)
        .await
        .expect("machine B replaces it");
    assert_eq!(
        other.as_deref(),
        Some("desk"),
        "the replaced machine went unnamed"
    );
    assert_eq!(stored(&core).await.name, None, "machine A's name went to B");

    let unnamed = store_redeemed(&core, KEY_A.to_owned(), &ticket(TICKET_A), 8181)
        .await
        .expect("machine A replaces B");
    assert_eq!(unnamed.as_deref(), Some(UNNAMED_PAIRED));
}

/// The key a request takes comes with the name the record has for that
/// machine, which is what a refusal of it is said in.
#[tokio::test]
async fn the_credentials_carry_the_name_the_record_holds() {
    let core = paired_with_a(Some("desk")).await;

    let Ok(credentials) = far_credentials(Some(&stored(&core).await), FINGERPRINT_A) else {
        panic!("machine A's own key was withheld from machine A");
    };

    assert_eq!(credentials.name.as_deref(), Some("desk"));
    let far = FarProxy::new("http://127.0.0.1:9/v1", &credentials).unwrap();
    assert_eq!(far.shown_name(), "desk");
    assert_eq!(far.far_machine().name, "desk");
}
