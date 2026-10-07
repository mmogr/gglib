//! A `gglib` command against a model identifier that matches nothing.
//!
//! Every command that cannot go on without its model resolves the identifier
//! through one door, so all of them fail the same way. Two of them used to
//! exit 0 — `inspect` after printing to stderr, `remove` after printing to
//! stdout — which made a missing model invisible to a script checking the
//! exit code. `benchmark` and `config default` used to ask the library
//! themselves, and worded the miss as `model not found: x` and `Validation
//! error: Model not found: x`.
//!
//! Unlike `daemon_lifecycle`, these bind nothing: the lookup fails before any
//! daemon or port is involved, so they run in CI rather than behind `--ignored`.
//!
//! `up --model` and `serve` take an identifier too and go through the same
//! door, but each looks for llama.cpp first, and offers to install it, which a
//! test must not. `up`'s lookup is tested beside it, in `handlers/up`, and the
//! door itself in `handlers/model/resolver_tests.rs`.

use std::process::Command;

/// An identifier no library holds.
const MISSING: &str = "__no_such_model__";

/// Every command tested here, as typed after `gglib`.
///
/// `model retag` also accepts `--all`, and `model check-updates` spells its
/// identifier as a flag; both still route through the same resolver.
const IDENTIFIER_COMMANDS: &[&[&str]] = &[
    &["model", "inspect", MISSING],
    &["model", "inspect", MISSING, "--json"],
    &["model", "remove", MISSING, "--force"],
    &["model", "update", MISSING, "--name", "x", "--force"],
    &["model", "retag", MISSING],
    &["model", "verify", MISSING],
    &["model", "repair", MISSING],
    &["model", "upgrade", MISSING],
    &["model", "capabilities", MISSING],
    &["model", "explain", MISSING],
    &["model", "check-updates", "--identifier", MISSING],
    &["benchmark", "compare", "--prompt", "hi", "--model", MISSING],
    &["benchmark", "perf", "--model", MISSING],
    &["benchmark", "tune", "--model", MISSING],
    &["benchmark", "agentic", "--model", MISSING],
    &["config", "default", MISSING],
    &["question", "hi", "--model", MISSING],
    &["chat", MISSING],
];

#[test]
fn an_unknown_identifier_fails_the_same_way_everywhere() {
    let dir = tempfile::tempdir().expect("temp data dir");

    for args in IDENTIFIER_COMMANDS {
        let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
            .args(*args)
            .env("GGLIB_DATA_DIR", dir.path())
            .output()
            .unwrap_or_else(|e| panic!("running `gglib {}`: {e}", args.join(" ")));

        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let label = args.join(" ");

        // The hazard this guards: `Ok(())` here means `gglib model remove x`
        // succeeds against a model that does not exist.
        assert_eq!(
            out.status.code(),
            Some(1),
            "`gglib {label}` must exit 1 for an unknown identifier\n\
             stdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stderr.contains("No model found matching: '__no_such_model__'"),
            "`gglib {label}` must name the identifier on stderr, got: {stderr}"
        );
        assert!(
            stderr.contains("Use 'gglib model list'"),
            "`gglib {label}` must keep the hint, got: {stderr}"
        );
        assert!(
            !stderr.to_lowercase().contains("model not found"),
            "`gglib {label}` worded the miss a second way, got: {stderr}"
        );
        assert!(
            stdout.is_empty(),
            "`gglib {label}` must write nothing to stdout, got: {stdout}"
        );
    }
}
