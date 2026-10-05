//! The Thinking choice on the wire: the key a turn carries when it says
//! one, and the key an opened chat's settings carry when it remembers.

use serde_json::json;

use super::HubTurn;
use super::hub_chats_tests::recorded;
use crate::domain::Thinking;
use crate::domain::chat::ConversationSettings;

/// The recorded turn that turns thinking off says so in one word beside its
/// message, and reads back.
#[test]
fn a_turn_that_says_off_carries_the_word_beside_its_message() {
    let body = serde_json::to_value(&recorded().thinking_turn).unwrap();
    assert_eq!(
        body,
        json!({ "conversation_id": 12, "content": "Answer in one line.", "thinking": "off" })
    );
    let read: HubTurn = serde_json::from_value(body).unwrap();
    assert_eq!(read.thinking, Some(Thinking::Off));
}

/// A turn that says nothing of thinking has no such key, so a hub that
/// predates the key reads it, and one read without the key says nothing.
#[test]
fn a_turn_that_says_nothing_leaves_the_key_out() {
    let body = serde_json::to_value(&recorded().turn).unwrap();
    assert!(body.get("thinking").is_none(), "{body}");
    let bare: HubTurn =
        serde_json::from_value(json!({ "conversation_id": 12, "content": "x" })).unwrap();
    assert_eq!(bare.thinking, None);
}

/// The two words are `off` and `default`, in lower case; a turn that says
/// any other is not a turn.
#[test]
fn a_turn_says_off_or_default_and_no_other_word() {
    for (word, choice) in [("off", Thinking::Off), ("default", Thinking::Default)] {
        assert_eq!(serde_json::to_value(choice).unwrap(), json!(word));
        let said = json!({ "conversation_id": 12, "content": "x", "thinking": word });
        let turn: HubTurn = serde_json::from_value(said).unwrap();
        assert_eq!(turn.thinking, Some(choice));
    }
    for other in [json!("on"), json!("Off"), json!(""), json!(0), json!(false)] {
        let said = json!({ "conversation_id": 12, "content": "x", "thinking": other });
        assert!(serde_json::from_value::<HubTurn>(said).is_err(), "{other}");
    }
}

/// The recorded chat remembers `off` beside its other settings. One that
/// remembers nothing has no key, and a row saved before the key reads as
/// remembering nothing.
#[test]
fn a_chat_that_remembers_off_says_so_in_its_settings() {
    let open = serde_json::to_value(&recorded().open).unwrap();
    assert_eq!(
        open["conversation"]["settings"],
        json!({ "max_iterations": 8, "thinking": "off" })
    );
    let nothing = serde_json::to_value(ConversationSettings::default()).unwrap();
    assert_eq!(nothing, json!({}));
    let older: ConversationSettings =
        serde_json::from_value(json!({ "max_iterations": 8 })).unwrap();
    assert_eq!(older.thinking, None);
}
