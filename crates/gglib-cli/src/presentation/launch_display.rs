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
//! the GUI's launch line is held to; and `contracts/models/unload_reason.json`:
//! the unload sentence for every reason, which the GUI's Load palette is
//! held to.

use std::fmt::Write as _;

use gglib_core::domain::{ContextFit, LaunchPlan, Unload, UnloadReason};

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
/// for the reason the plan gives, or nothing worth a line (nothing loaded,
/// or room beside what is).
#[must_use]
pub(crate) fn displacement_text(plan: &LaunchPlan) -> Option<String> {
    plan.unloads
        .as_ref()
        .map(|unload| format!("  {}", unload_sentence(unload)))
}

/// The sentence for a model a launch unloads first, and why. The GUI's
/// Load palette says the same words, held to them by
/// `contracts/models/unload_reason.json`.
#[must_use]
pub(crate) fn unload_sentence(unload: &Unload) -> String {
    let because = match unload.reason {
        UnloadReason::WouldNotFitBeside => "it would not fit beside it",
        UnloadReason::SecondSlotFull => "the second slot is taken",
        UnloadReason::OneImageModel => "one image model is loaded at a time",
    };
    format!("Unloads {} first: {because}.", unload.model)
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
/// Printed whatever the winning rung was. A fit that lost to a number
/// someone typed is still the fact that says whether the number was a good
/// one, and a fit that refused is the fact that explains the floor. The
/// gap between the trained window and `before snapping` is what the rung
/// ladder costs, which is the whole of ADR 0009's first kill criterion.
#[must_use]
pub(crate) fn context_text(fit: &ContextFit) -> String {
    let mut out = String::from("Context\n");
    let _ = writeln!(
        out,
        "  {:<22} {} ({})",
        "serves at",
        fit.serves_at,
        fit.source.label()
    );
    let _ = writeln!(out, "  {:<22} {}", "fitted to hardware", opt(fit.fitted));
    let _ = writeln!(out, "  {:<22} {}", "  device budget", gib(fit.budget_bytes));
    let _ = writeln!(out, "  {:<22} {}", "  weights", gib(fit.weights_bytes));
    let _ = writeln!(
        out,
        "  {:<22} {}",
        "  kv bytes/token",
        opt(fit.kv_bytes_per_token)
    );
    let _ = writeln!(out, "  {:<22} {}", "  trained window", opt(fit.trained_ctx));
    let _ = writeln!(out, "  {:<22} {}", "  before snapping", opt(fit.unsnapped));
    out
}

/// `None` reads as a refusal here, not as a zero — see `FitInputs`.
fn opt(v: Option<u64>) -> String {
    v.map_or_else(|| "unknown".to_owned(), |n| n.to_string())
}

/// Bytes as GiB, or `unknown` when the value could not be read.
fn gib(v: Option<u64>) -> String {
    v.map_or_else(
        || "unknown".to_owned(),
        |b| {
            #[allow(clippy::cast_precision_loss)]
            let g = b as f64 / (1024.0 * 1024.0 * 1024.0);
            format!("{g:.2} GiB")
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gglib_core::domain::{ChoiceSource, LaunchArg, LaunchNarration};
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
            unloads: Some(Unload {
                model: "qwen3-8b".to_owned(),
                reason: UnloadReason::WouldNotFitBeside,
            }),
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

        let second = LaunchPlan {
            unloads: Some(Unload {
                model: "nomic".to_owned(),
                reason: UnloadReason::SecondSlotFull,
            }),
            ..base.clone()
        };
        assert_eq!(
            displacement_text(&second).as_deref(),
            Some("  Unloads nomic first: the second slot is taken.")
        );
        let image = LaunchPlan {
            unloads: Some(Unload {
                model: "flux-a".to_owned(),
                reason: UnloadReason::OneImageModel,
            }),
            ..base.clone()
        };
        assert_eq!(
            displacement_text(&image).as_deref(),
            Some("  Unloads flux-a first: one image model is loaded at a time.")
        );

        let beside = LaunchPlan {
            unloads: None,
            ..base
        };
        assert_eq!(displacement_text(&beside), None);
    }

    /// Every figure is printed, a refusal as `unknown`, never as a zero.
    #[test]
    fn the_context_section_prints_the_chain_and_the_fits_inputs() {
        let fit = ContextFit {
            serves_at: 4096,
            source: ContextSizeSource::BuiltInDefault,
            fitted: None,
            budget_bytes: Some(96 << 30),
            weights_bytes: Some(4_284_481_536),
            kv_bytes_per_token: None,
            trained_ctx: None,
            unsnapped: None,
        };
        assert_eq!(
            context_text(&fit),
            "Context\n  serves at              4096 (built-in default)\n  fitted to hardware     unknown\n    device budget        96.00 GiB\n    weights              3.99 GiB\n    kv bytes/token       unknown\n    trained window       unknown\n    before snapping      unknown\n"
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

    /// One row of `contracts/models/unload_reason.json`: a reason, the
    /// model it names, and the sentence the CLI prints for them.
    #[derive(serde::Serialize)]
    struct RecordedUnload {
        unload: Unload,
        sentence: String,
    }

    /// The checked-in file is the unload sentence for every reason, in
    /// declaration order. Run with `GGLIB_RECORD_CONTRACTS=1` to rewrite it
    /// after a deliberate change.
    #[test]
    fn the_checked_in_unload_sentences_are_the_clis() {
        let recorded: Vec<RecordedUnload> = UnloadReason::ALL
            .into_iter()
            .map(|reason| {
                let unload = Unload {
                    model: "Qwen3-30B-A3B".to_owned(),
                    reason,
                };
                RecordedUnload {
                    sentence: unload_sentence(&unload),
                    unload,
                }
            })
            .collect();
        let mut want = serde_json::to_string_pretty(&recorded).expect("serialise");
        want.push('\n');
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../contracts/models/unload_reason.json");
        if std::env::var_os("GGLIB_RECORD_CONTRACTS").is_some() {
            std::fs::write(&path, &want).expect("write unload_reason.json");
        }
        let have =
            std::fs::read_to_string(&path).expect("read contracts/models/unload_reason.json");
        assert!(
            have == want,
            "contracts/models/unload_reason.json is stale; rerun with GGLIB_RECORD_CONTRACTS=1\n{want}"
        );
    }
}
