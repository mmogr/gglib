//! Draws the launch half of `gglib model explain`: the command gglib would
//! run, each argument beside whose choice it was and why, and the context
//! the model would be served at with the fit it was sized by.
//!
//! Format-only, like the rest of [`crate::presentation`]: every value comes
//! from the [`LaunchPlan`] the explain call answers with, the one
//! `GET /api/models/{id}/explain` sends, and nothing here resolves a flag,
//! a source or a figure of its own.

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
    out
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
}
