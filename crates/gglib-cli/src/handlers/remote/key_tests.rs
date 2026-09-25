//! Unit tests for [`super::write_key`]: which stream each line reaches.

use gglib_core::RemotePairing;

use super::write_key;

const KEY: &str = "device-key-5f3a9c";

fn pairing() -> RemotePairing {
    RemotePairing {
        ticket: "pipeabc".to_owned(),
        api_key: KEY.to_owned(),
        default_model: None,
        port: Some(8180),
    }
}

/// What `write_key` wrote to stdout and to stderr, and whether it failed.
fn run(pairing: Option<&RemotePairing>, show: bool) -> (String, String, anyhow::Result<()>) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let result = write_key(pairing, show, &mut out, &mut err);
    (
        String::from_utf8(out).expect("stdout is UTF-8"),
        String::from_utf8(err).expect("stderr is UTF-8"),
        result,
    )
}

#[test]
fn show_prints_the_key_alone_on_stdout() {
    let (out, _, result) = run(Some(&pairing()), true);

    result.expect("a stored pairing is shown");
    assert_eq!(out, format!("{KEY}\n"));
}

#[test]
fn show_warns_on_stderr_that_the_key_is_a_secret_and_keeps_the_key_off_it() {
    let (_, err, result) = run(Some(&pairing()), true);

    result.expect("a stored pairing is shown");
    assert!(err.contains("secret"), "stderr must warn: {err}");
    assert!(!err.contains(KEY), "the key must not reach stderr: {err}");
}

#[test]
fn without_show_stdout_stays_empty_and_stderr_says_how_to_print_the_key() {
    let (out, err, result) = run(Some(&pairing()), false);

    result.expect("a bare `key` with a pairing succeeds");
    assert_eq!(out, "", "nothing reaches stdout without --show");
    assert!(
        err.contains("`gglib remote key --show`"),
        "stderr must say how to print it: {err}"
    );
    assert!(!err.contains(KEY), "the key must not reach stderr: {err}");
}

#[test]
fn with_no_pairing_nothing_is_printed_and_the_refusal_names_join_and_the_proxy_key() {
    for show in [true, false] {
        let (out, err, result) = run(None, show);

        let refusal = result.expect_err("no pairing is a failure").to_string();
        assert_eq!((out.as_str(), err.as_str()), ("", ""), "show={show}");
        assert!(
            refusal.contains("`gglib remote join <ticket>-<code>`"),
            "the refusal must name the command that pairs: {refusal}"
        );
        assert!(
            refusal.contains("`proxy-api-key`"),
            "the refusal must name the proxy's own key: {refusal}"
        );
    }
}
