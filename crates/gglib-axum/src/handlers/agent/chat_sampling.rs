//! What a chat saved of how its turns sample, which a turn on it carries to
//! the model: [`ChatSampling`].

/// What a chat saved of how its turns sample (`gglib chat`'s flags): five
/// sampling settings, sent as a client's, which the endpoint honours only
/// while it trusts clients' sampling; and a profile, named on the model the
/// turn asks for (`{id}:{profile}`), which the endpoint applies whatever it
/// trusts.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ChatSampling {
    /// The saved `temperature`, `top_p`, `top_k`, `max_tokens` and
    /// `repeat_penalty`, any of them; `None` when the chat saved none.
    pub sampling: Option<gglib_core::domain::InferenceConfig>,
    /// The saved profile's name.
    pub profile: Option<String>,
}

impl ChatSampling {
    /// What `settings` saved of how the chat samples.
    pub(crate) fn of(settings: &gglib_core::domain::chat::ConversationSettings) -> Self {
        let sampling = gglib_core::domain::InferenceConfig {
            temperature: settings.temperature,
            top_p: settings.top_p,
            top_k: settings.top_k,
            max_tokens: settings.max_tokens,
            repeat_penalty: settings.repeat_penalty,
            ..gglib_core::domain::InferenceConfig::default()
        };
        let named = sampling != gglib_core::domain::InferenceConfig::default();
        Self {
            sampling: named.then_some(sampling),
            profile: settings.profile.clone().filter(|p| !p.trim().is_empty()),
        }
    }

    /// The model a turn asks the endpoint for: `model`, with the chat's
    /// profile as its `{model}:{profile}` suffix when it saved one.
    pub(crate) fn model(&self, model: String) -> String {
        match &self.profile {
            Some(profile) => format!("{model}:{profile}"),
            None => model,
        }
    }
}
