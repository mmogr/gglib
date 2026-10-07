//! What `gglib chat --continue` says when a chat's Thinking choice sets a
//! typed budget aside, run as a person runs it.
//!
//! A chat switched off on the chat page or a paired device runs with a
//! thinking budget of `0` whatever `--reasoning-budget-tokens` says: the
//! chat's choice wins, by the rule the daemon reads a turn by, and the
//! library's `resume_thinking_tests` pin the budget a turn sends. Pinned
//! here is what the command says of it: one line, on stderr, and only when
//! the budget typed is not the one the chat runs with.
//!
//! Each session is on a `--port` nothing listens on, is sent no message, and
//! ends at the end of its empty input.

use std::net::TcpListener;
use std::path::Path;
use std::process::{Command, Stdio};

use gglib_core::domain::Thinking;
use gglib_core::domain::chat::{ConversationSettings, NewConversation};

#[path = "support/data_dir.rs"]
mod data_dir;

/// The notice, for a typed budget of `typed`.
fn notice(typed: i32) -> String {
    format!(
        "  This chat has Thinking switched off, so --reasoning-budget-tokens {typed} was not \
         applied. Switch Thinking back on from the chat page or a paired device."
    )
}

/// What one session printed.
struct Session {
    stdout: String,
    stderr: String,
}

impl Session {
    /// The lines of stderr that speak of the budget flag.
    fn notices(&self) -> Vec<&str> {
        let about_the_flag = |line: &&str| line.contains("--reasoning-budget-tokens");
        self.stderr.lines().filter(about_the_flag).collect()
    }
}

/// A loopback port nothing listens on.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    listener.local_addr().expect("its address").port()
}

/// `gglib chat <args> --port <a closed port>` over the data directory
/// `root`, with nothing on stdin: a session that starts, and ends unasked.
fn chat(root: &Path, args: &[&str]) -> Session {
    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .arg("chat")
        .args(args)
        .args(["--port", &closed_port().to_string()])
        .env("GGLIB_DATA_DIR", root)
        .stdin(Stdio::null())
        .output()
        .expect("running `gglib chat`");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        out.status.success(),
        "`gglib chat {}` must succeed\nstdout: {stdout}\nstderr: {stderr}",
        args.join(" ")
    );
    Session { stdout, stderr }
}

/// `gglib chat --continue <id> [--reasoning-budget-tokens <typed>]`, on a
/// chat on `qwen` that remembers `thinking`, alone in its data directory.
fn resumed(thinking: Option<Thinking>, typed: Option<&str>) -> Session {
    let root = tempfile::tempdir().expect("temp data dir");
    let settings = ConversationSettings {
        model_name: Some("qwen".to_owned()),
        thinking,
        ..ConversationSettings::default()
    };
    let chat_row = NewConversation {
        title: "t".to_owned(),
        settings: Some(settings),
        ..NewConversation::default()
    };
    let id = data_dir::save_chat(root.path(), chat_row).to_string();
    let mut args = vec!["--continue", id.as_str()];
    if let Some(typed) = typed {
        args.extend(["--reasoning-budget-tokens", typed]);
    }
    chat(root.path(), &args)
}

#[test]
fn a_chat_switched_off_says_once_on_stderr_that_the_typed_budget_was_not_applied() {
    for typed in [4096, -1] {
        let session = resumed(Some(Thinking::Off), Some(&typed.to_string()));

        assert_eq!(session.notices(), [notice(typed)], "{}", session.stderr);
    }
}

/// What a script reads is the same with the notice as without it.
#[test]
fn the_notice_adds_nothing_to_stdout() {
    let silent = resumed(Some(Thinking::Off), None);
    let told = resumed(Some(Thinking::Off), Some("4096"));

    assert_eq!(told.notices().len(), 1, "{}", told.stderr);
    assert!(told.stdout.contains("Session #1 saved."), "{}", told.stdout);
    assert_eq!(told.stdout, silent.stdout);
}

/// No budget typed, or the `0` the chat runs with anyway: nothing was set
/// aside.
#[test]
fn a_chat_switched_off_says_nothing_when_no_budget_was_set_aside() {
    for typed in [None, Some("0")] {
        let session = resumed(Some(Thinking::Off), typed);

        assert!(
            session.notices().is_empty(),
            "typed {typed:?}: {}",
            session.stderr
        );
    }
}

/// A chat that remembers nothing, and one whose row says `default`, run
/// with the budget typed.
#[test]
fn a_chat_not_switched_off_says_nothing_of_a_typed_budget() {
    for thinking in [None, Some(Thinking::Default)] {
        let session = resumed(thinking, Some("4096"));

        assert!(
            session.notices().is_empty(),
            "{thinking:?}: {}",
            session.stderr
        );
    }
}

#[test]
fn a_new_chat_says_nothing_of_a_typed_budget() {
    let root = tempfile::tempdir().expect("temp data dir");

    let session = chat(root.path(), &["qwen", "--reasoning-budget-tokens", "4096"]);

    assert!(session.notices().is_empty(), "{}", session.stderr);
}
