//! The `HuggingFace` token this process asks the Hub with.
//!
//! [`from_env`] is the one place `HF_TOKEN` is read. `CoreBootstrap::build`
//! calls it once when its caller hands it no token of its own, and hands the
//! token it wires, the caller's or the environment's, to the Hub client, to
//! the download manager and to [`AppCore`](crate::services::AppCore); a
//! caller that needs it later asks `AppCore`. So a surface built on the
//! shared bootstrap holds the environment's token unless its caller hands it
//! another.
//!
//! [`normalised`] is the one rule for what a value counts as, whichever of
//! the two it came from: the spaces around it are dropped, and a blank value
//! is no token.
//!
//! The token is a secret: it goes to the Hub as a request header, and to the
//! download helper in its environment. Whatever holds it must keep it out of
//! every log line and every error's text.

/// The token in this process's environment, `HF_TOKEN`, when it holds one.
#[must_use]
pub fn from_env() -> Option<String> {
    std::env::var("HF_TOKEN")
        .ok()
        .as_deref()
        .and_then(normalised)
}

/// `value` as a token, the way `huggingface_hub` reads `HF_TOKEN`: the spaces
/// around it are dropped, and an empty or blank value is no token.
///
/// An empty bearer token is refused by the Hub even for a public repository,
/// so a blank value, from the environment or from a caller, must not become
/// one.
#[must_use]
pub fn normalised(value: &str) -> Option<String> {
    crate::secret::held(Some(value)).map(|token| token.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::normalised;

    #[test]
    fn a_value_is_the_token() {
        assert_eq!(
            normalised("hf_fake_token").as_deref(),
            Some("hf_fake_token")
        );
    }

    #[test]
    fn the_spaces_around_a_token_are_dropped() {
        assert_eq!(
            normalised("  hf_fake_token\n").as_deref(),
            Some("hf_fake_token")
        );
    }

    #[test]
    fn an_empty_or_blank_value_is_no_token() {
        for value in ["", "  \t", "\n"] {
            assert_eq!(normalised(value), None, "{value:?}");
        }
    }
}
