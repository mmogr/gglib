//! `--profile` under `--remote`: whose profile list gets consulted.
//!
//! A profile is configured per machine, so the machine that serves the turn is
//! the one that resolves it. `chat` and `q` both used to resolve it here and
//! then drop it — `compose` discards the profile on the remote path, because a
//! local profile does not describe the far machine's sampling — so the turn ran
//! at that machine's defaults with no warning. The suffix form travels and is
//! resolved there; the flag has no wire form and is now refused.
//!
//! This exists because the unit tests over `select_for_upstream` cannot reach
//! the two call sites: `agent_chat::run` and `agent_question::execute` both
//! need a live `CliContext`, so a call site that stopped passing `--remote`
//! through would leave those tests green. Driving the binary is the only seam
//! that catches it.
//!
//! Unlike `daemon_lifecycle`, these bind nothing and need no daemon: the
//! refusal happens before any upstream is resolved.

use std::process::Command;

/// Both commands that compose an agent session, each naming a profile this
/// machine would happily resolve if it were the one serving the turn.
const INVOCATIONS: &[&[&str]] = &[
    &["chat", "qwen3", "--remote", "--profile", "coding"],
    &["q", "-m", "qwen3", "--remote", "--profile", "coding", "hi"],
];

#[test]
fn a_profile_flag_under_remote_is_refused_by_both_agent_commands() {
    let dir = tempfile::tempdir().expect("temp data dir");

    for args in INVOCATIONS {
        let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
            .args(*args)
            .env("GGLIB_DATA_DIR", dir.path())
            .output()
            .unwrap_or_else(|e| panic!("running `gglib {}`: {e}", args.join(" ")));

        let stderr = String::from_utf8_lossy(&out.stderr);
        let label = args.join(" ");

        assert_eq!(
            out.status.code(),
            Some(1),
            "`gglib {label}` must exit 1\nstderr: {stderr}"
        );
        // Not "no profile named 'coding'", which is what resolving it against
        // *this* machine's (empty) list would say — that message is the
        // regression, dressed as an error.
        assert!(
            stderr.contains("--remote runs the turn on the other one"),
            "`gglib {label}` must say which machine owns the profile, got: {stderr}"
        );
        // The suffix does travel, so the refusal hands over the form that works.
        assert!(
            stderr.contains("`qwen3:coding`"),
            "`gglib {label}` must name the suffix form, got: {stderr}"
        );
    }
}
