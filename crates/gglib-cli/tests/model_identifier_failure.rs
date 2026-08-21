//! `gglib model <cmd> <identifier>` against an identifier that matches nothing.
//!
//! Every subcommand that takes an identifier resolves it through one door, so
//! all of them fail the same way. Two of them used to exit 0 — `inspect` after
//! printing to stderr, `remove` after printing to stdout — which made a missing
//! model invisible to a script checking the exit code.
//!
//! Unlike `daemon_lifecycle`, these bind nothing: the lookup fails before any
//! daemon or port is involved, so they run in CI rather than behind `--ignored`.

use std::process::Command;

/// Every `gglib model` subcommand that takes an identifier.
///
/// `retag` also accepts `--all`, and `check-updates` spells its identifier as a
/// flag; both still route through the same resolver.
const IDENTIFIER_SUBCOMMANDS: &[&[&str]] = &[
    &["inspect", "__no_such_model__"],
    &["inspect", "__no_such_model__", "--json"],
    &["remove", "__no_such_model__", "--force"],
    &["update", "__no_such_model__", "--name", "x", "--force"],
    &["retag", "__no_such_model__"],
    &["verify", "__no_such_model__"],
    &["repair", "__no_such_model__"],
    &["upgrade", "__no_such_model__"],
    &["capabilities", "__no_such_model__"],
    &["explain", "__no_such_model__"],
    &["check-updates", "--identifier", "__no_such_model__"],
];

#[test]
fn an_unknown_identifier_fails_the_same_way_everywhere() {
    let dir = tempfile::tempdir().expect("temp data dir");

    for args in IDENTIFIER_SUBCOMMANDS {
        let out = Command::new(env!("CARGO_BIN_EXE_gglib"))
            .arg("model")
            .args(*args)
            .env("GGLIB_DATA_DIR", dir.path())
            .output()
            .unwrap_or_else(|e| panic!("running `gglib model {}`: {e}", args.join(" ")));

        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let label = args.join(" ");

        // The hazard this guards: `Ok(())` here means `gglib model remove x`
        // succeeds against a model that does not exist.
        assert_eq!(
            out.status.code(),
            Some(1),
            "`gglib model {label}` must exit 1 for an unknown identifier\n\
             stdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stderr.contains("No model found matching: '__no_such_model__'"),
            "`gglib model {label}` must name the identifier on stderr, got: {stderr}"
        );
        assert!(
            stderr.contains("Use 'gglib model list'"),
            "`gglib model {label}` must keep the hint, got: {stderr}"
        );
        assert!(
            stdout.is_empty(),
            "`gglib model {label}` must write nothing to stdout, got: {stdout}"
        );
    }
}
