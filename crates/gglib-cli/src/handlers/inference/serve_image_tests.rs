//! `gglib serve` on an image model: the chat flags it refuses, by name and
//! before anything is asked of the daemon; what its banner says; and the
//! one load it sends, whose refusal is printed in the proxy's words.

use std::io::Write as _;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use clap::Parser as _;
use gglib_core::domain::{
    ComponentRole, ImageFamily, LaunchDecision, LaunchNarration, ModelComponent, NewModel,
};

use super::*;
use crate::bootstrap::test_context;
use crate::commands::Commands;
use crate::daemon_client::STAND_IN_PORT;
use crate::handlers::agent_chat::sight::sight_tests::read_request;
use crate::parser::Cli;
use crate::target::Target;

/// Everything `gglib <argv>` hands `serve::execute`.
struct Serve {
    identifier: String,
    context: ContextArgs,
    options: ServeOptions,
    sampling: SamplingArgs,
    profile: Option<String>,
    mtp: MtpArgs,
    cache: CacheArgs,
    access: AccessArgs,
}

fn serve(argv: &[&str]) -> Serve {
    let cli = Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("{argv:?}: {e}"));
    let Some(Commands::Serve {
        identifier,
        context,
        options,
        sampling,
        profile,
        mtp,
        cache,
        access,
    }) = cli.command
    else {
        panic!("{argv:?} is not `serve`");
    };
    Serve {
        identifier,
        context,
        options,
        sampling,
        profile: profile.profile,
        mtp,
        cache,
        access,
    }
}

/// The chat flags `argv` typed, as the image path names them.
fn typed(argv: &[&str], profile: bool) -> Vec<&'static str> {
    let s = serve(argv);
    ChatFlags {
        context: &s.context,
        options: &s.options,
        sampling: &s.sampling,
        mtp: &s.mtp,
        profile,
    }
    .typed()
}

#[test]
fn no_chat_flag_typed_is_nothing_to_refuse() {
    assert!(typed(&["gglib", "serve", "flux"], false).is_empty());
    // The proxy's own flags configure the endpoint, not the model.
    assert!(
        typed(
            &[
                "gglib",
                "serve",
                "flux",
                "--port",
                "9000",
                "--cache",
                "--host",
                "127.0.0.1"
            ],
            false
        )
        .is_empty()
    );
}

#[test]
fn each_chat_flag_is_named() {
    assert_eq!(
        typed(&["gglib", "serve", "flux", "--ctx-size", "4096"], false),
        ["--ctx-size"]
    );
    assert_eq!(
        typed(&["gglib", "serve", "flux", "-c", "max"], false),
        ["--ctx-size"]
    );
    assert_eq!(
        typed(&["gglib", "serve", "flux", "--mlock"], false),
        ["--mlock"]
    );
    assert_eq!(
        typed(&["gglib", "serve", "flux", "--jinja"], false),
        ["--jinja"]
    );
    assert_eq!(
        typed(&["gglib", "serve", "flux", "--temperature", "0.7"], false),
        ["sampling flags"]
    );
    assert_eq!(typed(&["gglib", "serve", "flux"], true), ["a profile"]);
    assert_eq!(
        typed(
            &[
                "gglib",
                "serve",
                "flux",
                "--mtp-draft-n-max",
                "0",
                "--mtp-draft-p-min",
                "0.5"
            ],
            false
        ),
        ["--mtp-draft-n-max", "--mtp-draft-p-min"]
    );
}

/// A stand-in on the daemon's port that answers as some other program and
/// counts what it was asked: a serve that reached for the daemon leaves a
/// count behind.
fn counting_stand_in() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let asked = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&asked);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let _ = read_request(&mut stream);
            count.fetch_add(1, Ordering::SeqCst);
            let reply = r#"{"service":"something-else"}"#;
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            );
        }
    });
    (port, asked)
}

/// `gglib serve flux --ctx-size 4096` on a model that draws: refused by
/// name, before the runtime is looked for and before the daemon is asked.
#[tokio::test]
async fn serve_of_an_image_model_refuses_a_context_flag_before_the_daemon_is_asked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ctx = test_context(dir.path()).await;
    let mut new = NewModel::new(
        "flux".to_owned(),
        dir.path().join("flux.gguf"),
        12.0,
        chrono::Utc::now(),
    );
    new.image_family = Some(ImageFamily::Flux1);
    ctx.app.models().add(new).await.expect("registered");
    let s = serve(&["gglib", "serve", "flux", "--ctx-size", "4096"]);
    let (port, asked) = counting_stand_in();

    let refused = STAND_IN_PORT
        .scope(
            port,
            super::super::execute(
                &ctx,
                Target::Local,
                s.identifier,
                s.context,
                s.options,
                s.sampling,
                s.profile,
                s.mtp,
                s.cache,
                s.access,
                false,
            ),
        )
        .await
        .expect_err("a context flag means nothing to an image model");

    assert_eq!(
        refused.to_string(),
        "'flux' is an image model: --ctx-size set a chat model's launch and do not apply \
         to it. Run 'gglib serve flux' without them."
    );
    assert_eq!(asked.load(Ordering::SeqCst), 0, "the daemon was asked");
}

/// A Flux model with its VAE linked and its two text encoders not.
fn flux_with_a_vae() -> Model {
    let mut new = NewModel::new(
        "flux-schnell".to_owned(),
        "/m/flux.gguf".into(),
        12.0,
        chrono::Utc::now(),
    );
    new.image_family = Some(ImageFamily::Flux1);
    new.components = vec![ModelComponent {
        role: ComponentRole::Vae,
        path: "/m/ae.safetensors".into(),
    }];
    Model::stored(7, &new)
}

#[test]
fn the_banner_names_the_runtime_the_family_and_every_component() {
    let lines = banner_lines(&flux_with_a_vae());
    assert_eq!(lines[0], "  Using model: flux-schnell (ID: 7)");
    assert_eq!(lines[1], "  File: /m/flux.gguf");
    assert_eq!(lines[2], "  Runtime: stable-diffusion.cpp");
    assert_eq!(lines[3], "  Family: Flux.1");
    let components: Vec<&str> = lines[4..].iter().map(String::as_str).collect();
    let vae = format!("    {:<13}: /m/ae.safetensors", ComponentRole::Vae.label());
    assert!(components.contains(&vae.as_str()), "{lines:#?}");
    let missing = components
        .iter()
        .filter(|l| l.ends_with(": missing"))
        .count();
    assert_eq!(
        missing,
        ImageFamily::Flux1.recipe().components.len() - 1,
        "every role but the VAE reads missing: {lines:#?}"
    );
    assert!(
        !lines.iter().any(|l| l.contains("Context")),
        "no chat banner line: {lines:#?}"
    );
}

/// A stand-in proxy that answers its one request with `status` and `body`,
/// after reading it whole, and keeps the request line.
fn proxy_answering(status: &'static str, body: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let lines = Arc::clone(&seen);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let (line, _) = read_request(&mut stream);
            lines.lock().unwrap().push(line);
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: application/json\r\n\
                 content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, seen)
}

#[tokio::test]
async fn a_load_asks_for_the_model_by_its_encoded_name_and_reads_the_answer() {
    let (port, seen) = proxy_answering(
        "200 OK",
        r#"{"model":"flux/schnell","started":true,"context":0}"#,
    );

    let loaded = load(&reqwest::Client::new(), port, "flux/schnell", None)
        .await
        .expect("loaded");

    assert_eq!(loaded.model, "flux/schnell");
    assert!(loaded.started);
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["POST /v1/models/flux%2Fschnell/load HTTP/1.1"]
    );
    assert_eq!(loaded_line(&loaded), "  \u{2705} flux/schnell is loaded");
}

/// The proxy's code is for programs; a person reads its message.
#[tokio::test]
async fn a_refused_load_is_printed_in_words_not_as_its_code() {
    let (port, _) = proxy_answering(
        "503 Service Unavailable",
        r#"{"error":{"message":"Image models need stable-diffusion.cpp. Run 'gglib config sd install'.","type":"server_error","code":"image_runtime_not_installed"}}"#,
    );

    let refused = load(&reqwest::Client::new(), port, "flux", None)
        .await
        .expect_err("refused");

    assert_eq!(
        refused.to_string(),
        "Image models need stable-diffusion.cpp. Run 'gglib config sd install'."
    );
    assert!(!refused.to_string().contains("image_runtime_not_installed"));
}

#[tokio::test]
async fn a_refusal_that_is_not_an_envelope_is_shown_whole() {
    let (port, _) = proxy_answering("502 Bad Gateway", "upstream went away");

    let refused = load(&reqwest::Client::new(), port, "flux", None)
        .await
        .expect_err("refused");

    assert_eq!(
        refused.to_string(),
        "The proxy refused to load 'flux' (502 Bad Gateway): upstream went away"
    );
}

#[test]
fn the_narration_reads_as_its_headline_then_each_decision() {
    let mut narration = LaunchNarration::new("flux-schnell", None, 0);
    narration.push(LaunchDecision::new(
        "runtime",
        "stable-diffusion.cpp master-948-228c707",
        "sd-config.json",
    ));
    narration.push(LaunchDecision::bare("slot", "secondary"));
    assert_eq!(
        narration_lines(&narration),
        [
            "  flux-schnell",
            "    runtime   stable-diffusion.cpp master-948-228c707 (sd-config.json)",
            "    slot      secondary",
        ]
    );
}

#[test]
fn the_refusal_names_every_flag_typed() {
    assert_eq!(
        chat_flags_refusal("flux", &["--ctx-size", "--jinja"]),
        "'flux' is an image model: --ctx-size, --jinja set a chat model's launch and do not \
         apply to it. Run 'gglib serve flux' without them."
    );
}
