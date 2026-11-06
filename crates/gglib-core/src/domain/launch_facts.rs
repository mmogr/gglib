//! How a loaded model was launched and what it is doing, said once.
//!
//! A model in memory has a few facts every surface wants beside its name:
//! the context it serves, the size of its weights, its quant, what its
//! launch decided (the narration), and whether it is working right now. The
//! runtime knows them; this is the shape they travel in, on the admission
//! snapshot's resident rows and on the server list, so `gglib proxy
//! dashboard`, `gglib model list` and the GUI's Loaded rows read one record
//! and print one line.
//!
//! The flag line is composed here, from the narration's own words, so that
//! no surface composes it again: the GUI arranges it, the CLI prints it.

use serde::{Deserialize, Serialize};

use super::launch_narration::{LaunchNarration, format_gib};
use super::state_words::ResidencyState;

/// How a resident model was launched, and what it is doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct LaunchFacts {
    /// What it is doing there, as the one word every surface uses.
    pub state: ResidencyState,
    /// The context it serves, in tokens.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub context_size: u64,
    /// On-disk size of its weights, all shards, plus its projector.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub weights_bytes: u64,
    /// Its quantization label, when the catalog knows one.
    pub quantization: Option<String>,
    /// What its launch decided, decision by decision; `None` for a launch
    /// that recorded nothing (a resident restored from disk).
    pub narration: Option<LaunchNarration>,
    /// The one line that says the launch: quant, context, KV cache types,
    /// speculative decoding when on, size. Composed once, here.
    pub flag_line: String,
}

impl LaunchFacts {
    /// The facts for a resident serving `context_size` tokens with
    /// `weights_bytes` of weights, launched as `narration` says.
    #[must_use]
    pub fn new(
        state: ResidencyState,
        context_size: u64,
        weights_bytes: u64,
        quantization: Option<String>,
        narration: Option<LaunchNarration>,
    ) -> Self {
        let flag_line = flag_line(
            quantization.as_deref(),
            context_size,
            weights_bytes,
            narration.as_ref(),
        );
        Self {
            state,
            context_size,
            weights_bytes,
            quantization,
            narration,
            flag_line,
        }
    }
}

/// The launch as one line: `Q4_K_M · 32K · k=q8_0 v=q8_0 KV · mtp on · 17.4 GiB`.
///
/// Each segment comes from a fact the launch recorded and is dropped when the
/// fact is missing, never written as "unknown": a quant the catalog lacks, a
/// KV decision an image model's launch never made, speculative decoding that
/// is off or unmentioned. The KV decision's value is the narration's own
/// (`k=q8_0 v=q8_0`), without the cache size it may append after `->`, since
/// that size is the context section's to say.
#[must_use]
pub fn flag_line(
    quantization: Option<&str>,
    context_size: u64,
    weights_bytes: u64,
    narration: Option<&LaunchNarration>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(quant) = quantization.filter(|q| !q.is_empty()) {
        parts.push(quant.to_owned());
    }
    if context_size > 0 {
        parts.push(format_context(context_size));
    }
    if let Some(n) = narration {
        if let Some(kv) = n.decision("kv") {
            let types = kv.value.split(" ->").next().unwrap_or("").trim();
            if !types.is_empty() {
                parts.push(format!("{types} KV"));
            }
        }
        if let Some(mtp) = n.decision("mtp")
            && mtp.value.starts_with("on")
        {
            parts.push("mtp on".to_owned());
        }
    }
    if weights_bytes > 0 {
        parts.push(format_gib(weights_bytes));
    }
    parts.join(" \u{b7} ")
}

/// A context size as people say it: `32K` for 32768, `128K` for 131072,
/// `4096` when it is not a whole number of K.
#[must_use]
pub fn format_context(tokens: u64) -> String {
    if tokens >= 1024 && tokens % 1024 == 0 {
        format!("{}K", tokens / 1024)
    } else {
        tokens.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::launch_narration::LaunchDecision;

    fn narration() -> LaunchNarration {
        let mut n = LaunchNarration::new("qwen3-8b", Some("Q4_K_M".to_owned()), 5 << 30);
        n.push(LaunchDecision::new("ctx", "32768", "fitted"));
        n.push(LaunchDecision::new(
            "kv",
            "k=q8_0 v=q8_0 -> 1.5 GiB",
            "default",
        ));
        n.push(LaunchDecision::new("mtp", "on, 3 draft tokens", "model"));
        n
    }

    #[test]
    fn the_flag_line_says_quant_context_kv_mtp_and_size_from_the_launch() {
        let facts = LaunchFacts::new(
            ResidencyState::Loaded,
            32768,
            17 * (1 << 30) + (1 << 30) * 2 / 5,
            Some("Q4_K_M".to_owned()),
            Some(narration()),
        );
        assert_eq!(
            facts.flag_line,
            "Q4_K_M \u{b7} 32K \u{b7} k=q8_0 v=q8_0 KV \u{b7} mtp on \u{b7} 17.4 GiB"
        );
    }

    #[test]
    fn a_missing_fact_drops_its_segment_and_never_says_unknown() {
        // No quant, no narration, a context that is not a whole K.
        assert_eq!(flag_line(None, 4000, 3 << 30, None), "4000 \u{b7} 3.0 GiB");
        // An empty quant is a missing one.
        assert_eq!(flag_line(Some(""), 0, 0, None), "");
        // MTP off is not said; a KV decision without a size is taken whole.
        let mut n = LaunchNarration::new("m", None, 0);
        n.push(LaunchDecision::new("kv", "k=f16 v=f16", "default"));
        n.push(LaunchDecision::new("mtp", "off", "GGLIB_DISABLE_MTP"));
        assert_eq!(
            flag_line(None, 8192, 0, Some(&n)),
            "8K \u{b7} k=f16 v=f16 KV"
        );
    }

    #[test]
    fn a_context_is_said_in_k_only_when_it_is_a_whole_number_of_them() {
        assert_eq!(format_context(131_072), "128K");
        assert_eq!(format_context(1024), "1K");
        assert_eq!(format_context(1000), "1000");
        assert_eq!(format_context(4096), "4K");
    }
}
