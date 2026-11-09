//! The launch gglib would make for a model, said flag by flag.
//!
//! `gglib model explain` prints it and `GET /api/models/{id}/explain`
//! serves it: the program and its arguments, each argument with the
//! decision it belongs to, whose choice it was and why, the narration a
//! launch prints, and the context fit. The runtime builds it from the same
//! resolutions a launch runs (`gglib-runtime`'s `plan_launch`), so it
//! cannot describe a launch that would not happen.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::launch_narration::LaunchNarration;
use crate::server_config::ContextSizeSource;

/// Whose choice an argument is.
///
/// On the wire each source is its `snake_case` name; [`word`](Self::word)
/// is what a person reads. The flags gglib does not send are llama.cpp's
/// own defaults, and are not arguments, so they have no source here: the
/// narration's `sampling` line says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum ChoiceSource {
    /// Read from the model's own files: its weights, what its GGUF declares.
    ModelAuthor,
    /// gglib's decision: a default, a fit to this machine, a tag it read.
    Gglib,
    /// Yours: a setting, a per-model default, a flag on this launch, a file
    /// you linked, a switch in the environment.
    You,
}

impl ChoiceSource {
    /// Every source, in display order.
    pub const ALL: [Self; 3] = [Self::ModelAuthor, Self::Gglib, Self::You];

    /// The words a person reads.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::ModelAuthor => "model author",
            Self::Gglib => "gglib",
            Self::You => "you",
        }
    }
}

impl fmt::Display for ChoiceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// One argument of the launch: a flag, its value when it takes one, and
/// the decision behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LaunchArg {
    /// The flag as the server takes it, e.g. `-c` or `--cache-type-k`.
    pub flag: String,
    /// Its value, as the server takes it; `None` for a bare flag.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// The decision this argument belongs to: the [`LaunchNarration`]'s
    /// label where it has one (`ctx`, `kv`, `cache`, `mtp`), else a short
    /// stable key (`model`, `host`, `port`, `parallel`, `templates`,
    /// `reasoning`, `images`, `embeddings`, `mlock`, `parts`, `recipe`).
    /// A surface groups the arguments by it; treat it as wire contract.
    pub decision: String,
    /// Whose choice it was.
    pub source: ChoiceSource,
    /// Why, in a sentence a person reads beside the flag.
    pub reason: String,
}

/// The context the model would be served at, and the fit it was sized by.
///
/// Every figure comes from the fit a launch performs (`fit_context_explained`
/// through the runtime's budgets); `None` is a value the machine or the
/// model's header could not supply, never a zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct ContextFit {
    /// The context size the launch resolves to.
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
    /// The model's trained ceiling.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub trained_ctx: Option<u64>,
    /// The context that fit before snapping to a rung.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number | null"))]
    pub unsnapped: Option<u64>,
    /// The fit said in a sentence, from the figures above (composed by
    /// [`ContextFit::new`]):
    /// `Fits this machine with 32K context, of 128K trained.`
    pub sentence: String,
    /// What the served context needs against the budget, with the verdict:
    /// `fits (35.81 GiB of 48.00 GiB)`; `None` when the fit did not know
    /// every figure it works from.
    pub note: Option<String>,
}

/// Why a launch unloads the model it names first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum UnloadReason {
    /// The second slot refused this model beside the one loaded (too large
    /// for it, no headroom, or nothing known to judge by), so the primary's
    /// resident is swapped out.
    WouldNotFitBeside,
    /// The second slot is this model's place (granted it beside the primary,
    /// or the primary is empty), but the slot already holds a model, and
    /// there are two slots.
    SecondSlotFull,
    /// One image model is loaded at a time: this one takes the loaded image
    /// model's slot.
    OneImageModel,
}

impl UnloadReason {
    /// Every reason, in declaration order.
    pub const ALL: [Self; 3] = [
        Self::WouldNotFitBeside,
        Self::SecondSlotFull,
        Self::OneImageModel,
    ];
}

/// The model a launch would unload first, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Unload {
    /// The loaded model, by name.
    pub model: String,
    pub reason: UnloadReason,
}

/// The launch gglib would make for a model now, without making it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlan {
    /// The server that would run: `llama-server` or `sd-server`.
    pub program: String,
    /// Its arguments, in the order they would be given, the port spelled
    /// `auto` because it is taken when the launch happens.
    pub args: Vec<LaunchArg>,
    /// The decisions a launch prints, the same words.
    pub narration: LaunchNarration,
    /// The loaded model this launch would unload first, and why, by the
    /// rules placement follows. `None` when nothing is loaded, when this
    /// model is already loaded (it relaunches in place), when it would load
    /// beside what is, and for an image model too large for the only place
    /// it may take, which waits instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub unloads: Option<Unload>,
    /// The context and its fit; `None` for an image model, which has no
    /// context.
    pub fit: Option<ContextFit>,
}

impl LaunchPlan {
    /// The command line, one token per argument, a value with a space in
    /// it quoted: what a person would type to make the same launch.
    #[must_use]
    pub fn command_line(&self) -> String {
        let mut line = self.program.clone();
        for arg in &self.args {
            line.push(' ');
            line.push_str(&arg.flag);
            if let Some(value) = &arg.value {
                line.push(' ');
                line.push_str(&quote(value));
            }
        }
        line
    }
}

/// `value` as a shell token: as it is unless it holds a space or a quote,
/// then in single quotes.
fn quote(value: &str) -> String {
    if value.is_empty() || value.contains([' ', '\'', '"', '\t']) {
        format!("'{}'", value.replace('\'', "'\\''"))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg(flag: &str, value: Option<&str>) -> LaunchArg {
        LaunchArg {
            flag: flag.to_owned(),
            value: value.map(str::to_owned),
            decision: "model".to_owned(),
            source: ChoiceSource::Gglib,
            reason: String::new(),
        }
    }

    fn plan(args: Vec<LaunchArg>) -> LaunchPlan {
        LaunchPlan {
            program: "llama-server".to_owned(),
            args,
            narration: LaunchNarration::new("m", None, 0),
            unloads: None,
            fit: None,
        }
    }

    #[test]
    fn the_command_line_is_the_program_and_each_token_in_order() {
        let plan = plan(vec![
            arg("-m", Some("/models/qwen.gguf")),
            arg("--metrics", None),
            arg("--port", Some("auto")),
        ]);
        assert_eq!(
            plan.command_line(),
            "llama-server -m /models/qwen.gguf --metrics --port auto"
        );
    }

    #[test]
    fn a_value_with_a_space_or_a_quote_is_quoted_for_a_shell() {
        let plan = plan(vec![
            arg("-m", Some("/Users/m/Library/Application Support/g/a.gguf")),
            arg("--mmproj", Some("/it's/p.gguf")),
        ]);
        assert_eq!(
            plan.command_line(),
            "llama-server -m '/Users/m/Library/Application Support/g/a.gguf' --mmproj '/it'\\''s/p.gguf'"
        );
    }

    #[test]
    fn a_source_is_its_snake_case_name_on_the_wire_and_its_words_to_a_person() {
        assert_eq!(
            serde_json::to_string(&ChoiceSource::ModelAuthor).unwrap(),
            r#""model_author""#
        );
        assert_eq!(ChoiceSource::ModelAuthor.to_string(), "model author");
        assert_eq!(
            ChoiceSource::ALL.map(ChoiceSource::word),
            ["model author", "gglib", "you"]
        );
    }

    #[test]
    fn a_bare_flag_sends_no_value_key() {
        let json = serde_json::to_string(&arg("--metrics", None)).unwrap();
        assert!(!json.contains("value"), "{json}");
    }
}
