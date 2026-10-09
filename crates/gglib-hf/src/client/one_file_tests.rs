//! One file by its path: a head read with a `Range`, and a file looked up at
//! `paths-info`, through the port and on the wire.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use gglib_core::ports::huggingface::HfClientPort;
use serde_json::json;

use crate::client::DefaultHfClient;
use crate::client::tests::test_config;
use crate::config::HfClientConfig;
use crate::http::testing::{Asked, CannedResponse, FakeBackend};

use super::*;

/// Not a token of any account.
const FAKE_TOKEN: &str = "hf_fake_token_for_a_head";

/// The Qwen-Image 2.1 VAE's place in its repository, a file in a folder.
const VAE_PATH: &str = "vae/qwen_image_2.1_vae_bf16.safetensors";

/// A request as a Hub on the loopback read it: its head and its body.
struct Served {
    head: String,
    body: String,
}

/// A Hub that answers its one request with `status` and `body`, and hands
/// back that request.
fn hub(status: &'static str, body: Vec<u8>) -> (String, JoinHandle<Served>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let base_url = format!("http://{}/api/models", listener.local_addr().unwrap());
    let served = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("a request");
        // The whole request is read before the answer is written: a peer
        // that answers and closes on unread bytes resets the connection.
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
        let head = String::from_utf8(head).expect("a head is text");
        let length = header(&head, "content-length").map_or(0, |n| n.parse().unwrap());
        let mut request_body = vec![0_u8; length];
        stream.read_exact(&mut request_body).expect("the body");
        let answer = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/octet-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(answer.as_bytes()).expect("the answer");
        // The client may stop reading once it holds what it asked for.
        let _ = stream.write_all(&body);
        Served {
            head,
            body: String::from_utf8(request_body).expect("a form is text"),
        }
    });
    (base_url, served)
}

/// A client of the Hub at `base_url`, holding a token.
fn client(base_url: String) -> DefaultHfClient {
    DefaultHfClient::new(&HfClientConfig {
        base_url,
        max_retries: 0,
        ..HfClientConfig::new().with_optional_token(Some(FAKE_TOKEN.to_owned()))
    })
}

/// The value of the request's `name` header, when it has one.
fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// A head is asked for with a `Range` of exactly its length, at the file's
/// resolve address, as the client's account.
#[tokio::test]
async fn a_head_is_asked_for_with_a_range_of_its_length() {
    let (base_url, served) = hub("206 Partial Content", vec![7; 16]);

    let head = HfClientPort::read_head(
        &client(base_url),
        "leejet/FLUX.1-schnell-gguf",
        "flux1-schnell-q8_0.gguf",
        16,
    )
    .await
    .expect("a head");

    assert_eq!(head, vec![7; 16]);
    let asked = served.join().unwrap().head;
    assert!(
        asked.starts_with("GET /leejet/FLUX.1-schnell-gguf/resolve/main/flux1-schnell-q8_0.gguf "),
        "{asked}"
    );
    assert_eq!(header(&asked, "range"), Some("bytes=0-15"));
    assert_eq!(
        header(&asked, "authorization"),
        Some(format!("Bearer {FAKE_TOKEN}").as_str())
    );
}

/// A server that ignores the `Range` and sends the whole file is read no
/// further than the head asked for.
#[tokio::test]
async fn a_server_that_sends_the_whole_file_is_cut_at_the_head() {
    let whole: Vec<u8> = (0..=255).collect();
    let (base_url, served) = hub("200 OK", whole.clone());

    let head = HfClientPort::read_head(
        &client(base_url),
        "leejet/FLUX.1-schnell-gguf",
        "flux1-schnell-q8_0.gguf",
        10,
    )
    .await
    .expect("a head");

    assert_eq!(head, whole[..10]);
    served.join().unwrap();
}

/// A file looked up by its path is posted to `paths-info` as a form naming
/// that path, and answered with its size and LFS OID.
#[tokio::test]
async fn a_file_is_looked_up_by_posting_its_path() {
    let answer = json!([{
        "type": "file",
        "path": VAE_PATH,
        "size": 675_509_688_u64,
        "oid": "a git object id",
        "lfs": {"oid": "the lfs sha256", "size": 675_509_688_u64, "pointerSize": 135}
    }]);
    let (base_url, served) = hub("200 OK", answer.to_string().into_bytes());

    let file = HfClientPort::file_at(&client(base_url), "Comfy-Org/Qwen-Image-2.1", VAE_PATH)
        .await
        .expect("an answer")
        .expect("the file");

    assert_eq!(file.path, VAE_PATH);
    assert_eq!(file.size, 675_509_688);
    assert_eq!(file.oid.as_deref(), Some("the lfs sha256"));
    assert!(!file.is_gguf);
    let request = served.join().unwrap();
    assert!(
        request
            .head
            .starts_with("POST /api/models/Comfy-Org/Qwen-Image-2.1/paths-info/main "),
        "{}",
        request.head
    );
    assert_eq!(
        header(&request.head, "content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        request.body,
        "paths=vae%2Fqwen_image_2.1_vae_bf16.safetensors"
    );
}

/// A path the repository does not hold is answered with an empty list, which
/// is no file and no error.
#[tokio::test]
async fn a_path_the_repository_does_not_hold_is_no_file() {
    let backend = FakeBackend::new().with_response(
        "paths-info",
        CannedResponse {
            json: json!([]),
            has_more: false,
        },
    );
    let client = HfClient::with_backend(test_config(), backend);
    let repo = HfRepoRef::new("Comfy-Org", "Qwen-Image-2.1");

    assert!(client.file_at(&repo, VAE_PATH).await.unwrap().is_none());
    assert_eq!(
        client.backend.asked(),
        [Asked::Form(
            "https://huggingface.co/api/models/Comfy-Org/Qwen-Image-2.1/paths-info/main".to_owned(),
            "paths=vae%2Fqwen_image_2.1_vae_bf16.safetensors".to_owned()
        )]
    );
}

/// A folder at the path is not a file, and nor is another path the answer
/// happens to carry.
#[tokio::test]
async fn a_folder_or_another_path_is_not_the_file() {
    let backend = FakeBackend::new().with_response(
        "paths-info",
        CannedResponse {
            json: json!([
                {"type": "directory", "path": "vae", "size": 0},
                {"type": "file", "path": "vae/other.safetensors", "size": 3}
            ]),
            has_more: false,
        },
    );
    let client = HfClient::with_backend(test_config(), backend);
    let repo = HfRepoRef::new("Comfy-Org", "Qwen-Image-2.1");

    assert!(client.file_at(&repo, "vae").await.unwrap().is_none());
    assert!(client.file_at(&repo, VAE_PATH).await.unwrap().is_none());
}

/// A head is read from the resolve address for the length asked, and a file
/// shorter than that answers all of itself.
#[tokio::test]
async fn a_short_file_answers_all_of_itself() {
    let backend = FakeBackend::new().with_body("resolve/main", b"GGUF");
    let client = HfClient::with_backend(test_config(), backend);
    let repo = HfRepoRef::new("leejet", "FLUX.1-schnell-gguf");

    let head = client
        .read_head(&repo, "flux1-schnell-q8_0.gguf", 1 << 20)
        .await
        .unwrap();

    assert_eq!(head, b"GGUF");
    assert_eq!(
        client.backend.asked(),
        [Asked::Head(
            "https://huggingface.co/leejet/FLUX.1-schnell-gguf/resolve/main/flux1-schnell-q8_0.gguf"
                .to_owned(),
            1 << 20
        )]
    );
}
