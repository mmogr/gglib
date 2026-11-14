//! What a chat saved of how its turns sample, which a turn on it carries to
//! the model: [`ChatSampling`].

/// What a chat saved of how its turns sample (`gglib chat`'s flags): a
/// token limit, sent as a client's budget, which the endpoint honours from
/// any client; and a profile, named on the model the turn asks for
/// (`{id}:{profile}`), which the endpoint applies whatever it trusts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ChatSampling {
    /// The saved `max_tokens`.
    pub max_tokens: Option<u32>,
    /// The saved profile's name.
    pub profile: Option<String>,
}

impl ChatSampling {
    /// What `settings` saved of how the chat samples.
    pub(crate) fn of(settings: &gglib_core::domain::chat::ConversationSettings) -> Self {
        Self {
            max_tokens: settings.max_tokens,
            profile: settings.profile.clone().filter(|p| !p.trim().is_empty()),
        }
    }

    /// The chat's profile, when it is still configured in `settings`. One
    /// that no longer is is dropped, and said once in the log: the chat's
    /// turns run without it rather than each being refused for it.
    pub(crate) fn configured_profile<'a>(
        &self,
        settings: &'a gglib_core::Settings,
    ) -> Option<&'a gglib_core::domain::InferenceProfile> {
        static SAID: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
        let name = self.profile.as_deref()?;
        let profiles = settings.inference_profiles.as_deref().unwrap_or_default();
        let found = profiles.iter().find(|p| p.name == name);
        let first = found.is_none() && {
            let mut said = SAID
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let unsaid = !said.iter().any(|n| n == name);
            if unsaid {
                said.push(name.to_owned());
            }
            unsaid
        };
        if first {
            tracing::warn!(
                profile = %name,
                "a chat's saved profile is no longer configured; its turns run without it"
            );
        }
        found
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

#[cfg(test)]
#[path = "chat_sampling_tests.rs"]
mod chat_sampling_tests;
