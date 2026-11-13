//! What the runtime decided for one llama-server launch, and why.
//!
//! ## Why this type exists
//!
//! gglib makes a series of non-obvious choices every time it launches a
//! model: it sizes the host-RAM prompt cache against free memory, quantizes
//! the KV cache, enables speculative decoding from the model's own metadata,
//! and resolves the context size through a five-level fallback chain. Every
//! one of those values already existed at launch — but only in a `debug!`
//! line or a local variable, so the user's evidence that any of it happened
//! was a 1,000-line README.
//!
//! [`LaunchNarration`] is the record of those decisions, captured once at
//! spawn where each resolver's `*Source` enum is still in scope. It carries
//! **provenance, not just values**: "32768" is a number, "32768 (model
//! `server_defaults`)" is an explanation, and the second is the one that makes
//! the runtime's behaviour auditable rather than magical.
//!
//! ## One record, three surfaces
//!
//! The same narration is rendered by the CLI banner at startup, served on
//! `GET /v1/gglib/proxy/status`, and displayed in the GUI dashboard. It lives in
//! `gglib-core` so all three can name the type without any of them depending
//! on the runtime that produces it.
//!
//! ## Presentation, not computation
//!
//! Nothing here re-derives a value. Every field is assigned from a resolution
//! the launch already performed; a decision gglib does not actually make has
//! no business appearing in this struct. Notably absent is the GPU layer
//! split: gglib never emits `-ngl`, so how many layers get offloaded is
//! llama.cpp's decision and is not gglib's to report. See
//! the `backend` [`LaunchDecision`] for what *is* known.

use serde::{Deserialize, Serialize};

use super::launch_plan::ContextFit;
use crate::server_config::ContextSizeSource;

/// One resolved launch decision, paired with the reason it was chosen.
///
/// [`Self::source`] is the whole point of the type — a value without its
/// provenance tells a user what happened but never why, which is precisely
/// the gap this record closes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct LaunchDecision {
    /// Short stable key naming the decision: `ctx`, `backend`, `kv`,
    /// `cache`, `mtp`, `flags`.
    ///
    /// Stable because the GUI and the CLI dashboard both key styling off it;
    /// treat it as part of the wire contract rather than a display string.
    pub label: String,
    /// Display-ready value, e.g. `32768` or `q8_0 -> 2.1 GiB, f16 would be 4.2 GiB`.
    pub value: String,
    /// Where the value came from, rendered in parentheses by every consumer.
    ///
    /// `None` only for decisions whose value already states its own origin —
    /// never as a shortcut for "didn't bother", since an unexplained value is
    /// the exact failure mode this type exists to prevent.
    pub source: Option<String>,
}

impl LaunchDecision {
    /// A decision whose provenance is worth stating.
    #[must_use]
    pub fn new(
        label: impl Into<String>,
        value: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            source: Some(source.into()),
        }
    }

    /// A decision that is self-explanatory — see [`Self::source`] for when
    /// that is legitimate.
    #[must_use]
    pub fn bare(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            source: None,
        }
    }
}

/// Everything the runtime decided for one llama-server launch.
///
/// Built at spawn by the runtime; consumed by the CLI banner, the proxy
/// status endpoint, and the GUI dashboard. See the [module docs](self) for
/// why the decisions are an ordered list rather than named fields: the three
/// consumers all render the same rows in the same order, and a list keeps
/// adding a decision to a single site instead of four. The fit is a record of
/// its own ([`LaunchFit`]): the status route carries all of it, the GUI shows
/// its sentence and figures, and the banner one row
/// ([`LaunchFit::banner_row`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct LaunchNarration {
    /// Model name as the catalog knows it.
    pub model_name: String,
    /// Quantization label (`Q4_K_M`), when the catalog recorded one.
    pub quantization: Option<String>,
    /// On-disk weight size in bytes, summed across shards. `0` when unknown —
    /// rendered as absent rather than as "0 GiB".
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub weights_bytes: u64,
    /// The decisions, in the order they should be displayed.
    pub decisions: Vec<LaunchDecision>,
    /// The context fit the launch was sized by, the record
    /// `gglib model explain` shows for a launch not yet made. `None` for an
    /// image model, which has no context, and for a launch made with the fit
    /// switched off (`GGLIB_DISABLE_CONTEXT_FIT`); a hub older than the field
    /// leaves it out.
    #[serde(default)]
    pub fit: Option<LaunchFit>,
}

/// The context fit a launch was sized by, as its narration carries it.
///
/// Every field of [`ContextFit`], unchanged, under `snake_case` keys like the
/// rest of the narration (a plan's own `fit` keeps `ContextFit`'s
/// `camelCase`). Converted from the launch's `ContextFit`, never computed
/// again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct LaunchFit {
    /// The context size the launch resolved to.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub serves_at: u64,
    /// Which rung of the chain supplied it.
    pub source: ContextSizeSource,
    /// The context fitted to this machine, whatever rung won; `None` is a
    /// refusal, with the fields below saying what was missing.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub fitted: Option<u64>,
    /// Device memory the fit was allowed to spend against.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub budget_bytes: Option<u64>,
    /// The weights, summed across shards, plus the projector when linked.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub weights_bytes: Option<u64>,
    /// Bytes of KV cache each token of context costs at the launch's types.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub kv_bytes_per_token: Option<u64>,
    /// The KV cache at the served context.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub kv_bytes: Option<u64>,
    /// The model's trained ceiling.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub trained_ctx: Option<u64>,
    /// The context that fit before snapping to a rung.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub unsnapped: Option<u64>,
    /// The fit said in a sentence:
    /// `Fits this machine with 32K context, of 128K trained.`
    pub sentence: String,
    /// What the served context needs against the share of the budget the
    /// fit spends, with the verdict; `None` when a figure it needs is.
    pub note: Option<String>,
    /// The figures above as a person reads them, one row each, `unknown`
    /// where one could not be read.
    pub figures: Vec<LaunchDecision>,
}

impl From<ContextFit> for LaunchFit {
    /// Field for field. Destructured in full, so a field added to
    /// [`ContextFit`] does not compile until it is carried here.
    fn from(fit: ContextFit) -> Self {
        let ContextFit {
            serves_at,
            source,
            fitted,
            budget_bytes,
            weights_bytes,
            kv_bytes_per_token,
            kv_bytes,
            trained_ctx,
            unsnapped,
            sentence,
            note,
            figures,
        } = fit;
        Self {
            serves_at,
            source,
            fitted,
            budget_bytes,
            weights_bytes,
            kv_bytes_per_token,
            kv_bytes,
            trained_ctx,
            unsnapped,
            sentence,
            note,
            figures,
        }
    }
}

impl LaunchFit {
    /// The fit as one row of the launch banner, which has room for one:
    /// `fit  40005 unsnapped  (of 48.0 GiB, 2.3 GiB weights, 1072
    /// KiB/token)`. The context row above it says the rung; this says what
    /// the rung was snapped from, and from what: the budget, the weights and
    /// the KV cost a token (rounded up to a KiB; the exact figure is in
    /// [`Self::figures`]). Under the banner's widest label it keeps to 80
    /// columns for a budget under 1000 GiB and weights under 100 GiB.
    ///
    /// `None` when the fit reached no figure (refused): the banner prints no
    /// row of unknowns, and the status route and the GUI carry the
    /// refusal's sentence.
    #[must_use]
    pub fn banner_row(&self) -> Option<LaunchDecision> {
        let unknown = || "unknown".to_owned();
        Some(LaunchDecision::new(
            "fit",
            format!("{} unsnapped", self.unsnapped?),
            format!(
                "of {}, {} weights, {} KiB/token",
                self.budget_bytes.map_or_else(unknown, format_gib),
                self.weights_bytes.map_or_else(unknown, format_gib),
                self.kv_bytes_per_token
                    .map_or_else(unknown, |b| b.div_ceil(1024).to_string())
            ),
        ))
    }
}

impl LaunchNarration {
    /// Start a narration for a model; decisions are appended by the runtime
    /// as each resolution comes into scope.
    #[must_use]
    pub fn new(
        model_name: impl Into<String>,
        quantization: Option<String>,
        weights_bytes: u64,
    ) -> Self {
        Self {
            model_name: model_name.into(),
            quantization,
            weights_bytes,
            decisions: Vec::new(),
            fit: None,
        }
    }

    /// Append a decision, keeping display order.
    pub fn push(&mut self, decision: LaunchDecision) {
        self.decisions.push(decision);
    }

    /// The identity line: `qwen3-30b-a3b · Q4_K_M · 17.2 GiB`.
    ///
    /// Unknown quantization and unknown size are dropped rather than rendered
    /// as `unknown` or `0 GiB` — a banner that pads itself with non-answers
    /// reads as broken.
    #[must_use]
    pub fn headline(&self) -> String {
        let mut parts = vec![self.model_name.clone()];
        if let Some(quant) = &self.quantization {
            parts.push(quant.clone());
        }
        if self.weights_bytes > 0 {
            parts.push(format_gib(self.weights_bytes));
        }
        parts.join(" \u{b7} ")
    }

    /// Look up a decision by its stable label.
    #[must_use]
    pub fn decision(&self, label: &str) -> Option<&LaunchDecision> {
        self.decisions.iter().find(|d| d.label == label)
    }
}

/// Format a byte count as GiB with one decimal, e.g. `17.2 GiB`.
///
/// GiB rather than GB, because the division is by 1024^3: it matches how every
/// other memory figure in the launch path is computed, and a banner whose
/// numbers disagree with the `--cache-ram` budget beside them is worse than no
/// banner.
#[must_use]
pub fn format_gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes_as_gib(bytes))
}

/// A byte count in GiB, for a figure a person reads: the one place the
/// cast is made.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub(crate) fn bytes_as_gib(bytes: u64) -> f64 {
    bytes as f64 / 1_073_741_824.0
}

/// Format a MiB count as GiB with one decimal, for the RAM cache budget.
#[must_use]
pub fn format_mib_as_gib(mib: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let gib = mib as f64 / 1024.0;
    format!("{gib:.1} GiB")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FitInputs, KvBytes};

    /// The narration's fit is the launch's `ContextFit`, every field of it,
    /// under `snake_case` keys: its JSON is the `ContextFit`'s with each key
    /// respelled and nothing added, dropped or changed.
    #[test]
    fn a_launch_fit_carries_every_field_of_the_context_fit() {
        let fit = ContextFit::new(
            32_768,
            ContextSizeSource::FittedToHardware,
            Some(32_768),
            &FitInputs {
                budget_bytes: Some(48 << 30),
                weights_bytes: Some(2_500_000_000),
                kv: Some(KvBytes::uniform(1_097_000)),
                trained_ctx: Some(131_072),
                unsnapped: Some(40_005),
            },
        );
        let snake = |key: &str| {
            key.chars().fold(String::new(), |mut out, c| {
                if c.is_ascii_uppercase() {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
                out
            })
        };
        let serde_json::Value::Object(camel) = serde_json::to_value(&fit).unwrap() else {
            panic!("a fit is an object");
        };
        let want: serde_json::Map<String, serde_json::Value> =
            camel.into_iter().map(|(k, v)| (snake(&k), v)).collect();
        assert_eq!(want.len(), 12, "a field went missing from the fit itself");
        assert!(want.contains_key("kv_bytes_per_token"));
        assert_eq!(
            serde_json::to_value(LaunchFit::from(fit)).unwrap(),
            serde_json::Value::Object(want)
        );
    }

    #[test]
    fn headline_joins_the_three_identity_parts() {
        let n = LaunchNarration::new("qwen3-30b-a3b", Some("Q4_K_M".to_string()), 18_476_297_420);
        assert_eq!(n.headline(), "qwen3-30b-a3b \u{b7} Q4_K_M \u{b7} 17.2 GiB");
    }

    /// Unknown quant and unknown size drop out rather than rendering as
    /// filler — see [`LaunchNarration::headline`].
    #[test]
    fn headline_drops_unknown_quantization_and_zero_size() {
        let n = LaunchNarration::new("mystery", None, 0);
        assert_eq!(n.headline(), "mystery");
    }

    #[test]
    fn headline_keeps_size_when_only_quantization_is_unknown() {
        let n = LaunchNarration::new("mystery", None, 1_073_741_824);
        assert_eq!(n.headline(), "mystery \u{b7} 1.0 GiB");
    }

    #[test]
    fn decisions_keep_insertion_order_and_are_findable_by_label() {
        let mut n = LaunchNarration::new("m", None, 0);
        n.push(LaunchDecision::new("ctx", "32768", "flag"));
        n.push(LaunchDecision::new("kv", "q8_0", "default"));
        let labels: Vec<&str> = n.decisions.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels, ["ctx", "kv"]);
        assert_eq!(n.decision("kv").unwrap().value, "q8_0");
        assert!(n.decision("mtp").is_none());
    }

    #[test]
    fn format_mib_as_gib_scales_by_1024() {
        assert_eq!(format_mib_as_gib(6144), "6.0 GiB");
    }
}
