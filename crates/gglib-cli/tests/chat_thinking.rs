//! `gglib chat` and a chat's Thinking choice, run as a person runs it: what
//! `--thinking` has the chat remember, and what a resume says when the
//! chat's choice sets a typed budget aside.
//!
//! A chat switched off, here or on the chat page or a paired device, runs
//! with a thinking budget of `0` whatever `--reasoning-budget-tokens` says,
//! until `--thinking on` or the page's switch sets it back: the rule is the
//! one the daemon reads a turn by, and the library's `resume_thinking_tests`
//! pin the budget a turn sends. Pinned here is the flag as it is typed, down
//! to what the chat stores, and what the command says of a budget: one line,
//! on stderr, and only when a typed budget was set aside by a choice the
//! command line did not name.
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

const OFF: Option<Thinking> = Some(Thinking::Off);

/// The notice, for a typed budget of `typed`.
fn notice(typed: i32) -> String {
    format!(
        "  This chat has Thinking switched off, so --reasoning-budget-tokens {typed} was not \
         applied. Add --thinking on to switch it back on."
    )
}

/// What one session printed, and what its chat remembers of thinking once
/// the session has ended.
struct Session {
    stdout: String,
    stderr: String,
    remembered: Option<Thinking>,
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
/// Its chat is the directory's first: the one it continues, or the one it
/// starts in a directory that holds none.
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
    let settings = data_dir::read_chat(root, 1).settings;
    Session {
        stdout,
        stderr,
        remembered: settings.and_then(|settings| settings.thinking),
    }
}

/// `gglib chat --continue <id> <flags>`, on a chat on `qwen` that remembers
/// `thinking`, alone in its data directory.
fn resumed(thinking: Option<Thinking>, flags: &[&str]) -> Session {
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
    args.extend(flags);
    chat(root.path(), &args)
}

/// `gglib chat qwen <flags>` in a data directory that holds no chat.
fn started(flags: &[&str]) -> Session {
    let root = tempfile::tempdir().expect("temp data dir");
    let mut args = vec!["qwen"];
    args.extend(flags);
    chat(root.path(), &args)
}

#[test]
fn a_chat_switched_off_says_once_on_stderr_that_the_typed_budget_was_not_applied() {
    for typed in [4096, -1] {
        let session = resumed(OFF, &["--reasoning-budget-tokens", &typed.to_string()]);

        assert_eq!(session.notices(), [notice(typed)], "{}", session.stderr);
    }
}

/// What a script reads is the same with the notice as without it.
#[test]
fn the_notice_adds_nothing_to_stdout() {
    let silent = resumed(OFF, &[]);
    let told = resumed(OFF, &["--reasoning-budget-tokens", "4096"]);

    assert_eq!(told.notices().len(), 1, "{}", told.stderr);
    assert!(told.stdout.contains("Session #1 saved."), "{}", told.stdout);
    assert_eq!(told.stdout, silent.stdout);
}

/// No budget typed, or the `0` the chat runs with anyway: nothing was set
/// aside.
#[test]
fn a_chat_switched_off_says_nothing_when_no_budget_was_set_aside() {
    for flags in [&[][..], &["--reasoning-budget-tokens", "0"]] {
        let session = resumed(OFF, flags);

        assert!(
            session.notices().is_empty(),
            "{flags:?}: {}",
            session.stderr
        );
    }
}

/// A chat that remembers nothing, and one whose row says `default`, run
/// with the budget typed.
#[test]
fn a_chat_not_switched_off_says_nothing_of_a_typed_budget() {
    for thinking in [None, Some(Thinking::Default)] {
        let session = resumed(thinking, &["--reasoning-budget-tokens", "4096"]);

        assert!(
            session.notices().is_empty(),
            "{thinking:?}: {}",
            session.stderr
        );
    }
}

#[test]
fn a_new_chat_says_nothing_of_a_typed_budget() {
    let session = started(&["--reasoning-budget-tokens", "4096"]);

    assert!(session.notices().is_empty(), "{}", session.stderr);
}

/// The choice is the command line's own: `on` applies the budget typed, and
/// `off` sets it aside because it was asked to.
#[test]
fn a_session_that_names_its_choice_says_nothing_of_a_typed_budget() {
    for remembered in [OFF, None] {
        for choice in ["on", "off"] {
            let flags = ["--thinking", choice, "--reasoning-budget-tokens", "4096"];

            let session = resumed(remembered, &flags);

            assert!(
                session.notices().is_empty(),
                "{remembered:?}, --thinking {choice}: {}",
                session.stderr
            );
        }
    }
}

/// `--thinking` switches the chat as the page's switch does, and a resume
/// without it leaves what the chat remembers alone.
#[test]
fn thinking_on_and_off_switch_what_a_resumed_chat_remembers_and_no_flag_keeps_it() {
    for (remembered, flags, after) in [
        (OFF, &["--thinking", "on"][..], None),
        (None, &["--thinking", "off"], OFF),
        (OFF, &["--thinking", "off"], OFF),
        (None, &["--thinking", "on"], None),
        (OFF, &[], OFF),
        (None, &[], None),
    ] {
        let session = resumed(remembered, flags);

        assert_eq!(session.remembered, after, "{remembered:?}, {flags:?}");
    }
}

#[test]
fn a_new_chat_started_with_thinking_off_remembers_it_and_any_other_remembers_nothing() {
    for (flags, after) in [
        (&["--thinking", "off"][..], OFF),
        (&["--thinking", "on"], None),
        (&[], None),
    ] {
        let session = started(flags);

        assert_eq!(session.remembered, after, "{flags:?}");
    }
}

/// The switch's two words: a turn's own word for `on` is not one of them.
#[test]
fn thinking_takes_on_or_off_and_refuses_any_other_word() {
    let root = tempfile::tempdir().expect("temp data dir");

    let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
        .args(["chat", "qwen", "--thinking", "default"])
        .env("GGLIB_DATA_DIR", root.path())
        .stdin(Stdio::null())
        .output()
        .expect("running `gglib chat`");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("possible values: on, off"), "{stderr}");
}
