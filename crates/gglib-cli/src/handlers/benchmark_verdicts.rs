//! The agentic report's three verdict blocks: what the A/A arm says about the
//! delta, the paired comparison, and whether the positive control moved.
//!
//! Split from `benchmark.rs` so the handler stays inside the complexity
//! ratchet's budget. Each block reads the report and prints; none of them
//! changes what the report says.

use gglib_core::domain::benchmark::{
    AgenticEvalReport, CONTROL_MIN_COMPOSITE_GAP, ControlVerdict, EFFECT_NOISE_RATIO,
};

use crate::presentation::style;

/// What the A/A arm says about the size of the delta just rendered.
///
/// Placed immediately under the axis table, because it is the sentence that
/// decides how the composite row should be read — not a footnote to it. A delta
/// of 0.082 above a drift of 0.031 is a finding; the same 0.082 above a drift
/// of 0.070 is a coin landing the same way twice, and the table alone cannot
/// tell them apart.
pub(super) fn render_noise_block(report: &AgenticEvalReport) {
    let Some(verdict) = report.effect_verdict() else {
        eprintln!();
        eprintln!(
            "  {MUTED}no A/A arm ran, so nothing here shows how much of the delta above is \
             drift — read it as a direction, not a magnitude{RESET}",
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
        return;
    };
    let replicate = report
        .raw_replicate
        .as_ref()
        .map_or(f64::NAN, |r| r.composite);
    let ratio = verdict
        .ratio()
        .map_or_else(|| "—".to_owned(), |r| format!("{r:.1}×"));

    // An unseeded run has no seed list to name, and "re-run on 0 disjoint
    // seeds" would describe an arm that did in fact run.
    let how = if report.raw_replicates.len() > 1 {
        format!(
            "re-run {n} times on disjoint seed sets",
            n = report.raw_replicates.len(),
        )
    } else if report.replicate_seeds.is_empty() {
        "re-run unseeded".to_owned()
    } else {
        format!(
            "re-run on {n} disjoint {seeds}",
            n = report.replicate_seeds.len(),
            seeds = super::plural(report.replicate_seeds.len(), "seed"),
        )
    };

    eprintln!();
    eprintln!(
        "  {MUTED}A/A: the raw arm {how} scored {replicate:.3} against its own {raw:.3}{RESET}",
        raw = report.raw.composite,
        MUTED = style::MUTED,
        RESET = style::RESET,
    );
    let over = match verdict.pairs() {
        0 | 1 => String::new(),
        pairs => format!(" (mean over {pairs} pairwise gaps)"),
    };
    if verdict.exceeds_noise() {
        eprintln!(
            "  {SUCCESS}effect exceeds drift{RESET}: the {effect:+.3} composite delta is {ratio} \
             the {noise:.3} this eval moves with nothing changed{over}.",
            effect = verdict.effect(),
            noise = verdict.noise(),
            SUCCESS = style::SUCCESS,
            RESET = style::RESET,
        );
    } else {
        eprintln!(
            "  {WARN}effect is within drift{RESET}: the {effect:+.3} composite delta is {ratio} \
             the {noise:.3} this eval moves with nothing changed{over}, under the {min:.0}× \
             needed to call it more than noise.",
            effect = verdict.effect(),
            noise = verdict.noise(),
            min = EFFECT_NOISE_RATIO,
            WARN = style::WARNING,
            RESET = style::RESET,
        );
        eprintln!(
            "  {WARN}That is unresolved, not absent — the fix is more seeds, not a different \
             conclusion.{RESET}",
            WARN = style::WARNING,
            RESET = style::RESET,
        );
    }
    // Printed on success as well as failure. A ratio computed from a
    // handful of gaps is the kind of number that gets quoted as though it
    // were a p-value, and the caveat has to travel with it — sized to the
    // run: the single-pair wording on a three-gap estimate misstates the
    // degrees of freedom in the caveat about degrees of freedom (caught on
    // the first live multi-pair run).
    let df = match verdict.pairs() {
        0 | 1 => "one A/A pair estimates that drift from a single degree of freedom".to_owned(),
        pairs => format!("{pairs} pairwise gaps back that drift estimate"),
    };
    eprintln!(
        "  {MUTED}{df} — this is a sanity ratio, not a significance test{RESET}",
        MUTED = style::MUTED,
        RESET = style::RESET,
    );
}

/// The paired view: the same cells the delta above averages, compared as
/// matched pairs — which is what removes the eval's identical-arm spread
/// from the comparison.
pub(super) fn render_paired_block(report: &AgenticEvalReport) {
    let Some(paired) = report.paired_effect() else {
        return;
    };
    eprintln!();
    let p = paired.p_value.map_or_else(
        || {
            format!(
                "too few non-tied pairs for a p — read {wins}W against {losses}L directly",
                wins = paired.wins,
                losses = paired.losses,
            )
        },
        |p| format!("Wilcoxon one-sided p = {p:.4}"),
    );
    eprintln!(
        "  paired: {wins}W–{losses}L–{ties}T over {pairs} (task, seed) {pair_word}, \
         mean Δ {mean:+.3} on tool-match; {p}",
        wins = paired.wins,
        losses = paired.losses,
        ties = paired.ties,
        pairs = paired.pairs,
        pair_word = super::plural(paired.pairs, "pair"),
        mean = paired.mean_delta,
    );
    if paired.unmeasured_pairs > 0 {
        eprintln!(
            "  {WARN}{n} {pairs} dropped: at least one side never reached the model.{RESET}",
            n = paired.unmeasured_pairs,
            pairs = super::plural(paired.unmeasured_pairs, "pair"),
            WARN = style::WARNING,
            RESET = style::RESET,
        );
    }
}

/// The positive control's verdict.
///
/// Rendered **before** the efficiency numbers and never as a footnote: a
/// control that failed to move invalidates every delta above it, and a reader
/// scanning for the headline number has to meet that fact first.
pub(super) fn render_control_block(report: &AgenticEvalReport) {
    let Some(verdict) = report.control_verdict() else {
        // Not run. Distinct from "ran and failed", and said so rather than
        // left silent — the same rule the sampling readback applies to blind.
        eprintln!();
        eprintln!(
            "  {MUTED}no control arm ran, so nothing here shows whether this eval could have \
             detected a difference at all{RESET}",
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
        return;
    };
    let control = report.control.as_ref().map_or(f64::NAN, |c| c.composite);
    let gglib = report.gglib.composite;

    eprintln!();
    match verdict {
        ControlVerdict::Moved { gap } => eprintln!(
            "  {SUCCESS}control moved{RESET}: the deliberately broken sampling cost {gap:.3} \
             composite ({control:.3} vs {gglib:.3}), so this run can detect a sampling change.",
            SUCCESS = style::SUCCESS,
            RESET = style::RESET,
        ),
        ControlVerdict::TooSmall { gap } => {
            eprintln!(
                "  {DANGER}control did not move{RESET}: the deliberately broken sampling changed \
                 the composite by only {gap:.3} ({control:.3} vs {gglib:.3}), below the \
                 {min:.2} this apparatus needs to demonstrate sensitivity.",
                min = CONTROL_MIN_COMPOSITE_GAP,
                DANGER = style::DANGER,
                RESET = style::RESET,
            );
            eprintln!(
                "  {DANGER}Treat every delta above as uninterpretable: this run cannot tell \"no \
                 effect\" from \"no sensitivity\".{RESET}",
                DANGER = style::DANGER,
                RESET = style::RESET,
            );
        }
        // Never worded as "barely moved". It moved a great deal, the wrong
        // way, which contradicts the control's premise rather than failing a
        // threshold — and the fix is to the control, not to the suite size.
        ControlVerdict::WrongDirection { gap } => {
            eprintln!(
                "  {DANGER}control moved the WRONG WAY{RESET}: the deliberately broken sampling \
                 scored {gap:.3} ABOVE the gglib arm ({control:.3} vs {gglib:.3}).",
                DANGER = style::DANGER,
                RESET = style::RESET,
            );
            eprintln!(
                "  {DANGER}Its sampling was chosen to be bad, so this contradicts the control's \
                 premise. Fix the control before reading any delta above.{RESET}",
                DANGER = style::DANGER,
                RESET = style::RESET,
            );
        }
    }

    // The control's composite is a coarser number than the two it is printed
    // beside, and nothing else on the line says so.
    let control_seeds = report.control.as_ref().map_or(0, |c| c.seeds);
    if control_seeds < report.gglib.seeds {
        eprintln!(
            "  {MUTED}measured on {control_seeds} of the run's {run_seeds} seeds — enough for a \
             gap this size, and it is the slowest arm in the eval{RESET}",
            run_seeds = report.gglib.seeds,
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
    }

    // What the control does *not* establish, said where it will be read. A
    // control that clears 0.5 licenses no claim about resolving 0.08 — that is
    // the A/A arm's job, and conflating them is the easiest misreading of this
    // whole report.
    if verdict.demonstrated_sensitivity()
        && let Some(effect) = report.effect_verdict()
    {
        eprintln!(
            "  {MUTED}that demonstrates sensitivity at {gap:.3}, not at the {effect:.3} measured \
             above — see the A/A line for that{RESET}",
            gap = match verdict {
                ControlVerdict::Moved { gap }
                | ControlVerdict::TooSmall { gap }
                | ControlVerdict::WrongDirection { gap } => gap,
            },
            effect = effect.effect().abs(),
            MUTED = style::MUTED,
            RESET = style::RESET,
        );
    }
}
