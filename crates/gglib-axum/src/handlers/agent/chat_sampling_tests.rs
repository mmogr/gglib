//! What a chat's saved settings give its turns of how they sample.

use gglib_core::domain::chat::ConversationSettings;

use super::*;

/// A blank profile is no profile: the turn names its model alone.
#[test]
fn a_blank_saved_profile_is_no_profile() {
    for blank in ["", "  "] {
        let settings = ConversationSettings {
            profile: Some(blank.to_owned()),
            ..ConversationSettings::default()
        };

        let saved = ChatSampling::of(&settings);

        assert_eq!(saved.profile, None, "{blank:?}");
        assert_eq!(saved.model("12".to_owned()), "12");
    }
}

/// A named profile rides on the model, and the saved token limit is read.
#[test]
fn a_named_profile_rides_on_the_model_and_the_token_limit_is_read() {
    let settings = ConversationSettings {
        profile: Some("coding".to_owned()),
        max_tokens: Some(512),
        ..ConversationSettings::default()
    };

    let saved = ChatSampling::of(&settings);

    assert_eq!(saved.model("12".to_owned()), "12:coding");
    assert_eq!(saved.max_tokens, Some(512));
    assert_eq!(
        ChatSampling::of(&ConversationSettings::default()).max_tokens,
        None
    );
}
