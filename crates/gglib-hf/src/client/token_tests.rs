//! The Hub token on the wire: sent as a bearer on each request, and in no
//! error.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use gglib_core::ports::huggingface::{HfClientPort, HfSearchOptions};

use super::*;

/// Not a token of any account.
const FAKE_TOKEN: &str = "hf_fake_token_for_a_test";

/// A Hub that answers its one request with `status` and `body`, and hands
/// back that request's head.
fn hub(status: &'static str, body: &'static str) -> (String, JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let base_url = format!("http://{}/api/models", listener.local_addr().unwrap());
    let served = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("a request");
        // The whole head is read before the answer is written: a peer that
        // answers and closes on unread bytes resets the connection.
        let mut head = Vec::new();
        let mut byte = [0_u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            assert_eq!(
                stream.read(&mut byte).expect("the head"),
                1,
                "the head ended"
            );
            head.push(byte[0]);
        }
        let answer = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(answer.as_bytes()).expect("the answer");
        String::from_utf8(head).expect("a head is text")
    });
    (base_url, served)
}

/// A client of the Hub at `base_url`, asking as `token`.
fn client(base_url: String, token: Option<&str>) -> DefaultHfClient {
    DefaultHfClient::new(&HfClientConfig {
        base_url,
        max_retries: 0,
        ..HfClientConfig::new().with_optional_token(token.map(str::to_owned))
    })
}

/// The value of the request's `Authorization` header, when it has one.
fn authorization(head: &str) -> Option<&str> {
    head.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("authorization")
            .then(|| value.trim())
    })
}

/// A search by a client that holds a token is asked as that account.
#[tokio::test]
async fn a_search_carries_the_clients_token_as_a_bearer() {
    let (base_url, served) = hub("200 OK", "[]");

    let found = client(base_url, Some(FAKE_TOKEN))
        .search(&HfSearchOptions::new().with_query("phi"))
        .await
        .expect("an empty page");

    assert!(found.items.is_empty());
    let head = served.join().unwrap();
    assert!(head.starts_with("GET /api/models?"), "{head}");
    assert_eq!(
        authorization(&head),
        Some(format!("Bearer {FAKE_TOKEN}").as_str())
    );
}

/// A client that holds no token sends no credential at all.
#[tokio::test]
async fn a_search_without_a_token_is_asked_as_nobody() {
    let (base_url, served) = hub("200 OK", "[]");

    client(base_url, None)
        .search(&HfSearchOptions::new())
        .await
        .expect("an empty page");

    assert_eq!(authorization(&served.join().unwrap()), None);
}

/// The Hub refusing a token is reported without the token: the error names
/// the request it was, and the token was a header of it, not a part of its
/// address.
#[tokio::test]
async fn a_refused_token_is_in_no_error_text() {
    let (base_url, served) = hub("401 Unauthorized", "{}");

    let refused = client(base_url, Some(FAKE_TOKEN))
        .search(&HfSearchOptions::new())
        .await
        .expect_err("a refusal");

    assert!(authorization(&served.join().unwrap()).is_some());
    let text = format!("{refused} {refused:?}");
    assert!(text.contains("Authentication required"), "{text}");
    assert!(!text.contains(FAKE_TOKEN), "{text}");
}
