//! What `gglib daemon status` says of the proxy, in the daemon's words, and
//! what `gglib daemon stop --remote` says about the machine it stops: its
//! name, as every other surface shows it, and never its fingerprint.

use gglib_core::domain::{EndpointState, KeyState, UNNAMED_PAIRED};

use super::{ProxyStatusDto, proxy_lines, question, stopping_line};

/// A listening proxy is reported where a client reaches it, pinned to its
/// model when it is, with whether a request needs the key; one that is not
/// listening is its state alone. The words are the daemon's, printed whole.
#[test]
fn the_proxy_report_says_where_it_is_reached_and_whether_a_key_is_needed() {
    let listening = ProxyStatusDto {
        running: true,
        state: EndpointState::Pinned,
        port: Some(8080),
        base_url: Some("http://127.0.0.1:8080/v1".to_owned()),
        key: Some(KeyState::Required),
        pinned_model: Some("qwen".to_owned()),
        pinned_to: Some("Pinned to qwen".to_owned()),
        pinned_model_id: Some(12),
    };
    assert_eq!(
        proxy_lines(&listening),
        [
            "  Proxy:   Pinned to qwen at http://127.0.0.1:8080/v1",
            "  API key: Required",
        ]
    );

    let open = ProxyStatusDto {
        state: EndpointState::On,
        key: Some(KeyState::NoneNeeded),
        pinned_model: None,
        pinned_to: None,
        pinned_model_id: None,
        ..listening
    };
    assert_eq!(
        proxy_lines(&open),
        [
            "  Proxy:   On at http://127.0.0.1:8080/v1",
            "  API key: None needed",
        ]
    );

    let unread = ProxyStatusDto { key: None, ..open };
    assert_eq!(
        proxy_lines(&unread),
        ["  Proxy:   On at http://127.0.0.1:8080/v1"],
        "a key the daemon could not read gets no line, not a guessed word"
    );

    let off = ProxyStatusDto {
        running: false,
        state: EndpointState::Off,
        port: None,
        base_url: None,
        key: None,
        pinned_model: None,
        pinned_to: None,
        pinned_model_id: None,
    };
    assert_eq!(proxy_lines(&off), ["  Proxy:   Off"]);
}

#[test]
fn a_remote_stop_names_the_machine_it_stops() {
    assert_eq!(
        question("desk"),
        "  This stops the gglib daemon on desk: its proxy, its models, its downloads."
    );
    assert_eq!(
        stopping_line("desk"),
        "  \u{1f6d1} The gglib daemon on desk is stopping, and this side is disconnected."
    );
    assert!(
        stopping_line(UNNAMED_PAIRED).contains("on the paired machine is stopping"),
        "{}",
        stopping_line(UNNAMED_PAIRED)
    );
}
