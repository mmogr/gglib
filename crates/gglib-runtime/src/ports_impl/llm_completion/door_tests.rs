//! A door's sentence for a refused key: a paired machine's names it and sends
//! the reader to pair again; this machine's sends nobody anywhere.

use super::Door;

fn door(name: Option<&str>) -> Door {
    Door {
        root_url: "http://127.0.0.1:9931".to_owned(),
        key: Some("k3y-5f3a".to_owned()),
        name: name.map(str::to_owned),
    }
}

#[test]
fn a_paired_machines_refused_key_names_it_and_says_pair_again() {
    assert_eq!(
        door(Some("desk"))
            .refusal(Some("invalid_api_key"))
            .as_deref(),
        Some(
            "desk is not admitting this device's key — it has either stopped trusting this \
             device, or a key rotation there is still reaching the tunnel, which clears itself \
             within a few seconds. If waiting does not fix it, pair again with a fresh `gglib \
             remote invite` there"
        )
    );
}

#[test]
fn this_machines_refused_key_says_to_send_again_and_names_no_pairing() {
    assert_eq!(
        door(None).refusal(Some("invalid_api_key")).as_deref(),
        Some(
            "this machine's endpoint did not admit its own key — the key was changed a moment \
             ago, and the endpoint takes up to 5 seconds to pick up a new one. Send again in a \
             few seconds"
        )
    );
}

#[test]
fn only_a_refused_key_has_a_sentence_of_its_own() {
    for name in [Some("desk"), None] {
        let door = door(name);
        assert_eq!(door.refusal(Some("backend_unreachable")), None);
        assert_eq!(door.refusal(None), None);
    }
}

#[test]
fn a_door_formatted_for_a_log_line_carries_no_key() {
    let shown = format!("{:?}", door(Some("desk")));
    assert!(!shown.contains("k3y-5f3a"), "{shown}");
    assert!(shown.contains("http://127.0.0.1:9931"), "{shown}");
}

/// A blank key is never sent: this machine's door keeps its place and loses
/// the key, and a paired machine's door with nothing to send is dropped with
/// its name, as a far machine with a blank key always was.
#[test]
fn a_blank_key_is_never_carried() {
    let blank = |name: Option<&str>| Door {
        key: Some("   ".to_owned()),
        ..door(name)
    };
    let adapter = super::super::LlmCompletionAdapter::new("http://127.0.0.1:0", None);
    let local = adapter.with_door(Some(blank(None)));
    let kept = local.door.as_ref().expect("this machine's door is kept");
    assert_eq!(kept.key, None);

    let adapter = super::super::LlmCompletionAdapter::new("http://127.0.0.1:0", None);
    assert!(adapter.with_door(Some(blank(Some("desk")))).door.is_none());

    let adapter = super::super::LlmCompletionAdapter::new("http://127.0.0.1:0", None);
    let far = adapter.with_door(Some(door(Some("desk"))));
    assert_eq!(far.door.and_then(|d| d.key).as_deref(), Some("k3y-5f3a"));
}
