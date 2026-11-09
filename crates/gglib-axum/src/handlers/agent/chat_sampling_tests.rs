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

/// A named profile rides on the model; settings that name no sampling give
/// no sampling layer, and any one of the five gives one.
#[test]
fn a_named_profile_rides_on_the_model_and_any_saved_setting_is_a_layer() {
    let settings = ConversationSettings {
        profile: Some("coding".to_owned()),
        top_k: Some(20),
        ..ConversationSettings::default()
    };

    let saved = ChatSampling::of(&settings);

    assert_eq!(saved.model("12".to_owned()), "12:coding");
    assert_eq!(saved.sampling.and_then(|s| s.top_k), Some(20));
    assert_eq!(
        ChatSampling::of(&ConversationSettings::default()).sampling,
        None
    );
}
