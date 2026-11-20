//! The notices a gglib proxy writes into a reply's stream as text, in place
//! of a reply that showed nothing.
//!
//! The proxy writes them and gglib's own agent reads the same stream, on
//! this machine or from a paired one, so the texts are defined here once:
//! the proxy sends them, and [`is_proxy_notice`] lets the agent tell one
//! from the model's reply. A paired machine may run another gglib, so a
//! changed text is not recognised in what that machine sends.

/// Prefix prepended to reasoning text that is promoted into the content
/// channel because the turn produced no visible output of its own.
///
/// A model that never closes its `<think>` block leaves a complete, often
/// correct answer stranded in `reasoning_content`, which clients that
/// collapse reasoning render as an empty response. Promoting it makes the turn usable;
/// the flag keeps the underlying degradation visible instead of silently
/// papering over it.
pub const REASONING_ONLY_NOTICE_PREFIX: &str = "\n\n⚠️ [proxy: reasoning-only response] ";

/// Diagnostic text synthesized and sent to the client when an upstream
/// streaming response completes without emitting a single visible frame.
///
/// Without this, a degenerate generation (the model producing zero tokens —
/// e.g. a wedged or context-overflowed llama-server) reaches the client as a
/// silent empty stream that the LLM Gateway reports as "the model returned an
/// empty response" with no cause. Emitting a visible notice turns that silent
/// failure into a diagnosable one.
pub const EMPTY_STREAM_NOTICE: &str = "⚠️ [proxy] The model produced no output for this request. The upstream \
     server may be overloaded or degraded — retry, and if it persists restart \
     the model.";

/// Whether `text`, one piece of a reply's text, is a proxy's own notice and
/// not the model's writing. A notice is sent whole, as one piece.
#[must_use]
pub fn is_proxy_notice(text: &str) -> bool {
    text.starts_with(REASONING_ONLY_NOTICE_PREFIX) || text.starts_with(EMPTY_STREAM_NOTICE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The texts as every released proxy sends them: a paired machine on
    /// another version is recognised by these words.
    #[test]
    fn the_notices_keep_their_words() {
        assert_eq!(
            REASONING_ONLY_NOTICE_PREFIX,
            "\n\n⚠️ [proxy: reasoning-only response] "
        );
        assert_eq!(
            EMPTY_STREAM_NOTICE,
            "⚠️ [proxy] The model produced no output for this request. The upstream server may \
             be overloaded or degraded — retry, and if it persists restart the model."
        );
    }

    #[test]
    fn a_notice_is_told_from_a_models_text() {
        let promoted = format!("{REASONING_ONLY_NOTICE_PREFIX}The answer is 42.");
        let empty = format!("{EMPTY_STREAM_NOTICE} (finish_reason: none)");
        assert!(is_proxy_notice(&promoted));
        assert!(is_proxy_notice(&empty));

        assert!(!is_proxy_notice("The answer is 42."));
        assert!(!is_proxy_notice(
            "The proxy says \"[proxy] The model produced no output\" when a reply is empty."
        ));
        assert!(!is_proxy_notice(""));
    }
}
