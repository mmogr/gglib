//! The `HuggingFace` token this process asks the Hub with.
//!
//! [`from_env`] is the one place it is read. `CoreBootstrap::build` calls it
//! once and hands the answer to the Hub client, to the download manager and
//! to [`AppCore`]; a caller that needs it later asks `AppCore`. So no surface
//! built on the shared bootstrap can be wired without it.
//!
//! The token is a secret: it goes to the Hub as a request header, and to the
//! download helper in its environment. Whatever holds it must keep it out of
//! every log line and every error's text.

use crate::services::AppCore;

/// The token in this process's environment, `HF_TOKEN`, when it holds one.
#[must_use]
pub fn from_env() -> Option<String> {
    token_in(std::env::var("HF_TOKEN").ok())
}

/// `value` as a token, the way `huggingface_hub` reads the same variable: the
/// spaces around it are dropped, and an empty or blank value is no token.
fn token_in(value: Option<String>) -> Option<String> {
    value
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

impl AppCore {
    /// This core, holding the token its process asks the Hub with.
    #[must_use]
    pub fn with_hf_token(mut self, token: Option<String>) -> Self {
        self.hf_token = token;
        self
    }

    /// The Hub token, for a request to `HuggingFace` that this core's own
    /// Hub client does not make: an upgrade's check and its download. `None`
    /// asks as nobody.
    #[must_use]
    pub fn hf_token(&self) -> Option<String> {
        self.hf_token.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::token_in;

    #[test]
    fn a_set_variable_is_the_token() {
        assert_eq!(
            token_in(Some("hf_fake_token".to_owned())).as_deref(),
            Some("hf_fake_token")
        );
    }

    #[test]
    fn the_spaces_around_a_token_are_dropped() {
        assert_eq!(
            token_in(Some("  hf_fake_token\n".to_owned())).as_deref(),
            Some("hf_fake_token")
        );
    }

    /// An empty bearer token is refused by the Hub even for a public
    /// repository, so a variable set to nothing must not become one.
    #[test]
    fn an_unset_empty_or_blank_variable_is_no_token() {
        for value in [None, Some(String::new()), Some("  \t".to_owned())] {
            assert_eq!(token_in(value.clone()), None, "{value:?}");
        }
    }
}
