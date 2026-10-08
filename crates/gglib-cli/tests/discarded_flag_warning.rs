//! `gglib q` with a sampling flag the ladder passes over, run as a person
//! runs it against a model server on `--port`.
//!
//! `--profile chat --presence-penalty 1.2` with no `--temperature`: the
//! profile sets the temperature, the penalty travels with it, and the flag is
//! gone, from the request the server receives too. The command says so on
//! stderr, once, when the question is sent, which is when the ladder is
//! folded; it says nothing when the flag took effect, and nothing under `-Q`.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, channel};

use gglib_core::domain::{DefaultsOrigin, InferenceConfig, InferenceProfile, NewModel};
use serde_json::{Value, json};

#[path = "support/data_dir.rs"]
mod data_dir;

const WARNING: &str = "  Warning: --presence-penalty did not take effect. Sampling penalties \
     travel with whichever layer sets the temperature; pass --temperature to set them together.";

/// One short reply, as llama-server streams it.
const REPLY: &str = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ok\"},\
     \"finish_reason\":null}]}\n\n\
     data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
     data: [DONE]\n\n";

/// Read one request whole, so the answer is not sent over unread bytes: its
/// request line and its body.
fn read_request(stream: &mut TcpStream) -> (String, String) {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).is_ok_and(|n| n == 1) {
        head.push(byte[0]);
    }
    let head = String::from_utf8_lossy(&head).into_owned();
    let length = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, value)| value.trim().parse().ok())
        .unwrap_or(0);
    let mut body = vec![0_u8; length];
    let _ = stream.read_exact(&mut body);
    let line = head.lines().next().unwrap_or_default().to_owned();
    (line, String::from_utf8_lossy(&body).into_owned())
}

/// A stand-in for llama-server on a loopback port: every request is answered
/// with [`REPLY`], and each completion request's body is handed over.
fn model_server() -> (u16, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let (received, completions) = channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let (line, body) = read_request(&mut stream);
            if line.starts_with("POST /v1/chat/completions") {
                let _ = received.send(body);
            }
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\
                 connection: close\r\n\r\n{REPLY}",
                REPLY.len()
            );
        }
    });
    (port, completions)
}

/// A data directory whose catalogue holds `served`, and `tuned` with a
/// temperature a person set on it, and whose settings hold a `chat` profile
/// that sets a temperature.
fn library() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp data dir");
    let path = std::path::PathBuf::from("/models/served.gguf");
    let model = NewModel::new("served".to_owned(), path, 7.0, chrono::Utc::now());
    data_dir::add_model(root.path(), model);
    let path = std::path::PathBuf::from("/models/tuned.gguf");
    let mut tuned = NewModel::new("tuned".to_owned(), path, 7.0, chrono::Utc::now());
    tuned.inference_defaults = Some(InferenceConfig {
        temperature: Some(0.55),
        ..InferenceConfig::default()
    });
    tuned.defaults_origin = Some(DefaultsOrigin::User);
    data_dir::add_model(root.path(), tuned);
    data_dir::write_settings(root.path(), |settings| {
        settings.inference_profiles = Some(vec![InferenceProfile {
            name: "chat".to_owned(),
            description: None,
            config: InferenceConfig {
                temperature: Some(0.8),
                ..InferenceConfig::default()
            },
            list_in_models: false,
        }]);
    });
    root
}

/// What `gglib q hi -m <model> --no-tools --port <a model server> <flags>`
/// wrote to stderr, once it has answered, and the request the server got.
fn asked(root: &Path, model: &str, flags: &[&str]) -> (String, Value) {
    let (port, completions) = model_server();
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(["q", "hi", "-m", model, "--no-tools"])
        .args(["--port", &port.to_string()])
        .args(flags)
        .env("GGLIB_DATA_DIR", root)
        .stdin(Stdio::null())
        .output()
        .expect("running `gglib q`");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success() && stdout.contains("ok"),
        "`gglib q {}` must answer\nstdout: {stdout}\nstderr: {stderr}",
        flags.join(" ")
    );
    let sent = completions.try_recv().expect("a completion request");
    (stderr, serde_json::from_str(&sent).expect("a JSON body"))
}

fn warnings(stderr: &str) -> Vec<&str> {
    let of_a_flag = |line: &&str| line.contains("did not take effect");
    stderr.lines().filter(of_a_flag).collect()
}

/// The flag is warned of once and is not in the request: the profile's
/// temperature is, or the one a person set on the model.
#[test]
fn a_penalty_typed_beside_a_layer_that_sets_the_temperature_is_warned_of_once_and_not_sent() {
    let root = library();

    let beside_a_profile = ["--profile", "chat", "--presence-penalty", "1.2"];
    let (stderr, sent) = asked(root.path(), "served", &beside_a_profile);
    assert_eq!(warnings(&stderr), [WARNING], "{stderr}");
    assert_eq!(sent["temperature"], json!(0.8_f32), "{sent}");
    assert_eq!(sent.get("presence_penalty"), None, "{sent}");

    let (stderr, sent) = asked(root.path(), "tuned", &["--presence-penalty", "1.2"]);
    assert_eq!(warnings(&stderr), [WARNING], "{stderr}");
    assert_eq!(sent["temperature"], json!(0.55_f32), "{sent}");
    assert_eq!(sent.get("presence_penalty"), None, "{sent}");
}

/// A penalty typed with its temperature is sent and warned of nothing; a
/// quiet question is warned of nothing, and its passed-over flag is still
/// not sent.
#[test]
fn a_flag_that_took_effect_and_a_quiet_question_are_warned_of_nothing() {
    let root = library();
    let penalty = ["--profile", "chat", "--presence-penalty", "1.2"];

    let with_its_temperature = [&penalty[..], &["--temperature", "0.5"]].concat();
    let (stderr, sent) = asked(root.path(), "served", &with_its_temperature);
    assert_eq!(warnings(&stderr), [""; 0], "{stderr}");
    assert_eq!(sent["presence_penalty"], json!(1.2_f32), "{sent}");

    let quiet = [&penalty[..], &["-Q"]].concat();
    let (stderr, sent) = asked(root.path(), "served", &quiet);
    assert_eq!(warnings(&stderr), [""; 0], "{stderr}");
    assert_eq!(sent.get("presence_penalty"), None, "{sent}");
}
