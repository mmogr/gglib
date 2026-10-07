//! Tests for [`super`]: the default context `gglib proxy` passes through,
//! the port its start body carries, and the port a start then answers.

use std::io::Write as _;
use std::net::TcpListener;

use clap::Parser as _;

use super::{resolve_default_context, start_body, start_on};
use crate::commands::Commands;
use crate::daemon_client::{DaemonHandle, STAND_IN_PORT, StartProxyBody, paths};
use crate::handlers::agent_chat::sight::sight_tests::read_request;
use crate::parser::Cli;
use gglib_core::Settings;
use gglib_core::settings::DEFAULT_PROXY_PORT;

// ── The default context ──────────────────────────────────────────────────

/// With nothing configured, the daemon is told nothing. A chain resolved
/// to a bare `u64` here would tell it the user had chosen 4096, and the
/// fitted rung ([#925]) would never be reached.
///
/// [#925]: https://github.com/mmogr/gglib/pull/925
#[test]
fn nothing_configured_sends_nothing() {
    let settings = Settings::default();
    assert_eq!(resolve_default_context(None, &settings).unwrap(), None);
}

#[test]
fn a_stored_setting_is_passed_through_untouched() {
    let settings = Settings {
        default_context_size: Some(16_384),
        ..Settings::default()
    };
    assert_eq!(
        resolve_default_context(None, &settings).unwrap(),
        Some(16_384)
    );
}

#[test]
fn the_flag_outranks_a_stored_setting() {
    let settings = Settings {
        default_context_size: Some(16_384),
        ..Settings::default()
    };
    assert_eq!(
        resolve_default_context(Some("8192"), &settings).unwrap(),
        Some(8192)
    );
}

/// A flag parsed with `.ok()` would drop `8k` here without a word, and the
/// daemon would size the context as though no flag had been passed.
#[test]
fn a_malformed_flag_is_an_error_not_a_shrug() {
    let err = resolve_default_context(Some("8k"), &Settings::default())
        .expect_err("a value that is not a number must not be discarded");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("8k"),
        "the message must name what was rejected: {msg}"
    );
}

/// Zero parses as a `u64`; the help calls it invalid. The message and the
/// behaviour have to agree.
#[test]
fn zero_is_rejected_because_the_message_calls_it_invalid() {
    resolve_default_context(Some("0"), &Settings::default())
        .expect_err("0 is outside the configurable range");
}

/// One number, three surfaces, one range. `validate_settings` rejects
/// below 512 and `--default-context-size` documents that bound, so this
/// flag accepting 1 would make them disagree.
#[test]
fn the_flag_is_held_to_the_same_range_the_settings_are() {
    use gglib_core::settings::CONTEXT_SIZE_RANGE;

    let below = CONTEXT_SIZE_RANGE.start() - 1;
    resolve_default_context(Some(&below.to_string()), &Settings::default())
        .expect_err("below the range must be rejected");

    let above = CONTEXT_SIZE_RANGE.end() + 1;
    resolve_default_context(Some(&above.to_string()), &Settings::default())
        .expect_err("above the range must be rejected");

    for edge in [*CONTEXT_SIZE_RANGE.start(), *CONTEXT_SIZE_RANGE.end()] {
        assert_eq!(
            resolve_default_context(Some(&edge.to_string()), &Settings::default()).unwrap(),
            Some(edge),
            "the range's own endpoints must be accepted"
        );
    }
}

/// Omitting the flag falls back to the stored setting, and only reaches
/// per-launch sizing when that is unset too. The message said "omit the
/// flag to fit the context to this machine", which is false for anyone
/// with a `default_context_size` stored — and disagreed with this flag's
/// own `--help`, which has always described the fallback correctly.
#[test]
fn the_remedy_describes_the_fallback_and_not_just_the_fit() {
    let err = resolve_default_context(Some("8k"), &Settings::default())
        .expect_err("`8k` is not a number this flag accepts");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("default_context_size"),
        "omitting the flag falls back to the setting; the message must say so: {msg}"
    );
    assert!(
        !msg.contains("fit the context to this machine"),
        "the message must not promise a fit that omitting the flag does not deliver: {msg}"
    );
}

/// The help says `max` is unsupported here; the error must agree with it
/// rather than recommending it.
#[test]
fn max_is_rejected_and_never_recommended() {
    let err = resolve_default_context(Some("max"), &Settings::default())
        .expect_err("`max` has no meaning for a proxy serving every model");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("not supported"),
        "the message must say `max` is unsupported: {msg}"
    );
    assert!(
        !msg.contains("or 'max'"),
        "the message must not offer `max` as the remedy for rejecting `max`: {msg}"
    );
}

// ── The port ─────────────────────────────────────────────────────────────

/// What a start sends and answers.
pub(in crate::handlers) struct Started {
    /// The request's body, as the daemon was sent it.
    pub sent: serde_json::Value,
    /// The port [`start_on`] answered.
    pub port: u16,
}

impl Started {
    /// Whether the body said nothing of a port: the key null or absent,
    /// which the daemon reads alike.
    pub(in crate::handlers) fn sent_no_port(&self) -> bool {
        self.sent.get("port").is_none_or(serde_json::Value::is_null)
    }
}

/// Start `body` through [`start_on`], against a stand-in that answers as a
/// daemon whose proxy is on `answer` would, or as one whose proxy is not
/// running when `answer` is `None`. `settings` is what is stored here.
pub(in crate::handlers) async fn started(
    body: &StartProxyBody,
    answer: Option<u16>,
    settings: &Settings,
) -> Started {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("its address").port();
    let daemon = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("the request");
        let request = read_request(&mut stream);
        let reply = serde_json::json!({ "running": answer.is_some(), "port": answer }).to_string();
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n{reply}",
            reply.len()
        );
        request
    });
    let handle = DaemonHandle {
        client: gglib_proxy::loopback::client(),
        api_key: None,
    };

    let answered = STAND_IN_PORT
        .scope(port, start_on(&handle, body, settings))
        .await
        .expect("the daemon's answer");

    let (line, sent) = daemon.join().expect("the stand-in ran");
    assert_eq!(line, format!("POST {} HTTP/1.1", paths::PROXY_START_PATH));
    Started {
        sent: serde_json::from_str(&sent).expect("a JSON body"),
        port: answered,
    }
}

/// The body `gglib proxy` sends for `argv`.
fn body_for(argv: &[&str]) -> StartProxyBody {
    let Some(Commands::Proxy {
        bind,
        sampling,
        cache,
        access,
        ..
    }) = Cli::parse_from(argv).command
    else {
        panic!("{argv:?} is not `gglib proxy`");
    };
    start_body(bind.host, bind.port, None, sampling, &cache, &access)
}

/// Without `--port` the daemon is sent no port, so its own fallback, the
/// stored `proxy_port`, decides where the proxy comes up; and the port
/// reported is the one it answers. A default filled in on this side is a port
/// somebody chose as far as the daemon can tell, and it outranks the setting
/// the desktop app and the tray honour.
#[tokio::test]
async fn without_a_port_flag_the_daemon_chooses_the_port_and_its_answer_is_reported() {
    let body = body_for(&["gglib", "proxy"]);

    let started = started(&body, Some(9000), &Settings::default()).await;

    assert!(started.sent_no_port(), "{}", started.sent);
    assert_eq!(started.port, 9000);
}

#[tokio::test]
async fn a_port_flag_is_sent_as_typed() {
    let body = body_for(&["gglib", "proxy", "--port", "8123"]);

    let started = started(&body, Some(8123), &Settings::default()).await;

    assert_eq!(started.sent["port"], 8123);
    assert_eq!(started.port, 8123);
}

/// The daemon's answer is the port, whatever was asked: a proxy that was
/// already running is where it is, not where this command wanted it.
#[tokio::test]
async fn a_proxy_already_running_elsewhere_is_reported_where_it_is() {
    let body = body_for(&["gglib", "proxy", "--port", "8123"]);

    let started = started(&body, Some(9000), &Settings::default()).await;

    assert_eq!(started.port, 9000);
}

/// A daemon that reports no port is not serving one. What is answered then
/// is where the start asked for it: the flag, else the stored port, else the
/// one default.
#[tokio::test]
async fn a_start_that_reports_no_port_answers_the_flag_then_the_stored_port() {
    let flagged = body_for(&["gglib", "proxy", "--port", "8123"]);
    let bare = body_for(&["gglib", "proxy"]);
    let stored = Settings {
        proxy_port: Some(9000),
        ..Settings::default()
    };

    assert_eq!(started(&flagged, None, &stored).await.port, 8123);
    assert_eq!(started(&bare, None, &stored).await.port, 9000);
    assert_eq!(
        started(&bare, None, &Settings::default()).await.port,
        DEFAULT_PROXY_PORT
    );
}
