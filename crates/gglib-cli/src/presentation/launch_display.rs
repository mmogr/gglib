//! Draws the launch half of `gglib model explain`: the command gglib would
//! run, each argument beside whose choice it was and why, and the context
//! the model would be served at with the fit it was sized by.
//!
//! Format-only, like the rest of [`crate::presentation`]: every value comes
//! from the [`LaunchPlan`] the explain call answers with, the one
//! `GET /api/models/{id}/explain` sends, and nothing here resolves a flag,
//! a source or a figure of its own.
//!
//! The tests also record `contracts/models/launch_line.json`: a few plans
//! beside the command line `LaunchPlan::command_line` makes of each, which
//! the GUI's launch line is held to.

use std::fmt::Write as _;

use gglib_core::domain::{ContextFit, LaunchPlan};

/// Width of the flag column: `--spec-draft-n-max` and `--reasoning-format`
/// are 18 characters, and the column keeps one space after them.
const FLAG_WIDTH: usize = 19;

/// Width of the value column. A path is longer and simply pushes its own
/// arrow right; `{:<N}` stops padding rather than truncating.
const VALUE_WIDTH: usize = 12;

/// The arrow separating a value from its source, as the sampling table's.
const ARROW: &str = "\u{2190}";

/// The launch and the context as text: the command line, a row per
/// argument, and the context section.
#[must_use]
pub(crate) fn launch_text(name: &str, id: i64, plan: &LaunchPlan) -> String {
    let mut out = format!(
        "\n  Launch for {name} (id {id})\n  {}\n\n",
        plan.command_line()
    );
    for line in argument_lines(plan) {
        let _ = writeln!(out, "  {line}");
    }
    if let Some(fit) = &plan.fit {
        out.push('\n');
        out.push_str(&context_text(fit));
    }
    if let Some(line) = displacement_text(plan) {
        out.push('\n');
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// What the launch would do to the models loaded now: unload the one named,
/// in the plan's own sentence (the one the GUI's Load palette shows), or
/// nothing worth a line (nothing loaded, or room beside what is).
#[must_use]
pub(crate) fn displacement_text(plan: &LaunchPlan) -> Option<String> {
    plan.unloads
        .as_ref()
        .map(|unload| format!("  {}", unload.sentence))
}

/// One string per argument: the flag, its value, and `← source: reason`.
#[must_use]
pub(crate) fn argument_lines(plan: &LaunchPlan) -> Vec<String> {
    plan.args
        .iter()
        .map(|arg| {
            format!(
                "{:<FLAG_WIDTH$} {:<VALUE_WIDTH$} {ARROW} {}: {}",
                arg.flag,
                arg.value.as_deref().unwrap_or(""),
                arg.source,
                arg.reason
            )
        })
        .collect()
}

/// The context chain, and what the fit worked from where it reached one.
///
/// It opens with the fit sentence the GUI shows under the model's name,
/// and the note its Context row says (what the served context needs of
/// the share of the budget the fit spends) when the budget, the weights
/// and the KV cost are known.
///
/// Printed whatever the winning rung was. A fit that lost to a number
/// someone typed is still the fact that says whether the number was a good
/// one, and a fit that refused is the fact that explains the floor. The
/// gap between the trained window and `before snapping` is what the rung
/// ladder costs, which is the whole of ADR 0009's first kill criterion.
#[must_use]
pub(crate) fn context_text(fit: &ContextFit) -> String {
    let mut out = format!("Context\n  {}\n", fit.sentence);
    let _ = writeln!(
        out,
        "  {:<22} {} ({})",
        "serves at",
        fit.serves_at,
        fit.source.label()
    );
    let _ = writeln!(out, "  {:<22} {}", "fitted to hardware", opt(fit.fitted));
    for figure in &fit.figures {
        let _ = writeln!(
            out,
            "  {:<22} {}",
            format!("  {}", figure.label),
            figure.value
        );
    }
    out
}

/// `None` reads as a refusal here, not as a zero — see `FitInputs`.
fn opt(v: Option<u64>) -> String {
    v.map_or_else(|| "unknown".to_owned(), |n| n.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::{
        ChoiceSource, FitInputs, KvBytes, LaunchArg, LaunchNarration, Unload, UnloadReason,
    };
    use gglib_core::server_config::ContextSizeSource;

    fn arg(flag: &str, value: Option<&str>, source: ChoiceSource, reason: &str) -> LaunchArg {
        LaunchArg {
            flag: flag.to_owned(),
            value: value.map(str::to_owned),
            decision: "x".to_owned(),
            source,
            reason: reason.to_owned(),
        }
    }

    /// The flag column fits the widest flag the token list emits, with one
    /// space after it; a value column is as wide as the arrow needs.
    #[test]
    fn each_argument_is_its_flag_value_and_source_with_reason() {
        let plan = LaunchPlan {
            program: "llama-server".to_owned(),
            args: vec![
                arg(
                    "-m",
                    Some("/m/q.gguf"),
                    ChoiceSource::ModelAuthor,
                    "the weights",
                ),
                arg(
                    "--metrics",
                    None,
                    ChoiceSource::Gglib,
                    "the dashboard reads them",
                ),
                arg("--spec-draft-n-max", Some("2"), ChoiceSource::You, "set"),
            ],
            narration: LaunchNarration::new("q", None, 0),
            unloads: None,
            fit: None,
        };
        assert_eq!(
            argument_lines(&plan),
            [
                "-m                  /m/q.gguf    \u{2190} model author: the weights",
                "--metrics                        \u{2190} gglib: the dashboard reads them",
                "--spec-draft-n-max  2            \u{2190} you: set",
            ]
        );
        assert!(launch_text("q", 3, &plan).starts_with(
            "\n  Launch for q (id 3)\n  llama-server -m /m/q.gguf --metrics --spec-draft-n-max 2\n\n"
        ));
        // Nothing loaded, or room beside it: no line about unloading.
        assert!(!launch_text("q", 3, &plan).contains("Unload"));
    }

    /// A launch that would unload says which model goes and why, in the
    /// plan's reason; one that unloads nothing has no line.
    #[test]
    fn a_launch_that_would_unload_says_so_at_the_end() {
        let base = LaunchPlan {
            program: "llama-server".to_owned(),
            args: vec![],
            narration: LaunchNarration::new("q", None, 0),
            unloads: Some(Unload::new(
                "qwen3-8b".to_owned(),
                UnloadReason::WouldNotFitBeside,
            )),
            fit: None,
        };
        assert_eq!(
            displacement_text(&base).as_deref(),
            Some("  Unloads qwen3-8b first: it would not fit beside it.")
        );
        assert!(
            launch_text("q", 3, &base)
                .ends_with("  Unloads qwen3-8b first: it would not fit beside it.\n")
        );

        let beside = LaunchPlan {
            unloads: None,
            ..base
        };
        assert_eq!(displacement_text(&beside), None);
    }

    /// Every figure is printed, a refusal as `unknown`, never as a zero,
    /// under the fit sentence; a refusal that did not know the KV cost has
    /// no memory row.
    #[test]
    fn the_context_section_prints_the_chain_and_the_fits_inputs() {
        let fit = ContextFit::new(
            4096,
            ContextSizeSource::BuiltInDefault,
            None,
            &FitInputs {
                budget_bytes: Some(96 << 30),
                weights_bytes: Some(4_284_481_536),
                ..FitInputs::default()
            },
        );
        assert_eq!(
            context_text(&fit),
            "Context\n  Serves at 4K context; gglib could not fit one to this machine: the model does not say what its context costs or what it was trained for.\n  serves at              4096 (built-in default)\n  fitted to hardware     unknown\n    device budget        96.00 GiB\n    weights              3.99 GiB\n    kv bytes/token       unknown\n    kv cache             unknown\n    trained window       unknown\n    before snapping      unknown\n"
        );
    }

    /// A fit that knew every figure says it fits, and what the served
    /// context needs of the budget, the words the GUI shows.
    #[test]
    fn a_fit_prints_its_sentence_and_its_memory_row() {
        let fit = ContextFit::new(
            32_768,
            ContextSizeSource::FittedToHardware,
            Some(32_768),
            // The GUI fixtures' chat model: 2.5 GB of weights, 1,097,000
            // bytes of KV cache a token, a 128K window and 48 GiB, which fit
            // 40,005 tokens before snapping to 32K.
            &FitInputs {
                budget_bytes: Some(48 << 30),
                weights_bytes: Some(2_500_000_000),
                kv: Some(KvBytes::uniform(1_097_000)),
                trained_ctx: Some(131_072),
                unsnapped: Some(40_005),
            },
        );
        assert_eq!(
            context_text(&fit),
            "Context\n  Fits this machine with 32K context, of 128K trained.\n  serves at              32768 (fitted to hardware)\n  fitted to hardware     32768\n    memory               fits (35.81 GiB of 43.20 GiB usable)\n    device budget        48.00 GiB\n    weights              2.33 GiB\n    kv bytes/token       1097000\n    kv cache             33.48 GiB\n    trained window       131072\n    before snapping      40005\n"
        );
    }

    /// The plans `contracts/models/launch_line.json` records: an ordinary
    /// llama launch, one whose values need quoting for a shell (a space, a
    /// single quote, a double quote, a tab, an empty value, a `$`, a
    /// pattern with `\` and `*`) beside one that needs none for all its
    /// `=,+%:@`, and an image model's.
    fn recorded_plans() -> Vec<LaunchPlan> {
        use ChoiceSource::{Gglib, ModelAuthor, You};
        let plan = |program: &str, args: Vec<LaunchArg>| LaunchPlan {
            program: program.to_owned(),
            args,
            narration: LaunchNarration::new("m", None, 0),
            unloads: None,
            fit: None,
        };
        let a = |flag: &str, value: Option<&str>, decision: &str, source| LaunchArg {
            decision: decision.to_owned(),
            ..arg(flag, value, source, "why")
        };
        vec![
            plan(
                "llama-server",
                vec![
                    a("-m", Some("/models/qwen3-4b.gguf"), "model", ModelAuthor),
                    a("--host", Some("127.0.0.1"), "host", Gglib),
                    a("--port", Some("auto"), "port", Gglib),
                    a("--metrics", None, "metrics", Gglib),
                    a("--parallel", Some("1"), "parallel", Gglib),
                    a("-c", Some("32768"), "ctx", Gglib),
                    a("--jinja", None, "templates", Gglib),
                    a("--cache-type-k", Some("q8_0"), "kv", Gglib),
                    a("--cache-type-v", Some("q8_0"), "kv", Gglib),
                    a(
                        "--reasoning-format",
                        Some("deepseek"),
                        "reasoning",
                        ModelAuthor,
                    ),
                ],
            ),
            plan(
                "llama-server",
                vec![
                    a(
                        "-m",
                        Some("/Users/m/Library/Application Support/gglib/a b.gguf"),
                        "model",
                        ModelAuthor,
                    ),
                    a("--mmproj", Some("/it's/mmproj.gguf"), "images", You),
                    a("--alias", Some("say \"hi\""), "model", Gglib),
                    a("--sep", Some("a\tb"), "model", Gglib),
                    a("--empty", Some(""), "model", Gglib),
                    a("--alias", Some("$HOME"), "model", Gglib),
                    a("-ot", Some(r"blk\.\d+\.ffn_.*=CPU"), "model", Gglib),
                    a("--override-kv", Some("k=v,a+b%c:d@e"), "model", Gglib),
                ],
            ),
            plan(
                "sd-server",
                vec![
                    a(
                        "--diffusion-model",
                        Some("/m/flux1-schnell-q8_0.gguf"),
                        "model",
                        ModelAuthor,
                    ),
                    a("--vae", Some("/m/ae.safetensors"), "parts", You),
                    a("--listen-ip", Some("127.0.0.1"), "host", Gglib),
                    a("--listen-port", Some("auto"), "port", Gglib),
                    a("--steps", Some("4"), "recipe", Gglib),
                ],
            ),
        ]
    }

    /// One row of `contracts/models/launch_line.json`: a plan as the wire
    /// sends it, and the command line made of it.
    #[derive(serde::Serialize)]
    struct RecordedLine {
        plan: LaunchPlan,
        line: String,
    }

    /// The checked-in file is exactly what `command_line` makes of each
    /// plan. Run with `GGLIB_RECORD_CONTRACTS=1` to rewrite it after a
    /// deliberate change.
    #[test]
    fn the_checked_in_launch_lines_are_the_command_lines() {
        let recorded: Vec<RecordedLine> = recorded_plans()
            .into_iter()
            .map(|plan| RecordedLine {
                line: plan.command_line(),
                plan,
            })
            .collect();
        let mut want = serde_json::to_string_pretty(&recorded).expect("serialise");
        want.push('\n');
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/models/launch_line.json");
        if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).expect("make contracts/models");
            std::fs::write(&path, &want).expect("write launch_line.json");
        }
        let have = std::fs::read_to_string(&path).expect("read contracts/models/launch_line.json");
        assert!(
            have == want,
            "contracts/models/launch_line.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
        );
    }
}
