//! The paired statistic behind [`PairedEffect`]: matched `(task, seed)` cells,
//! their win/loss/tie counts, and a one-sided Wilcoxon signed-rank *p*.
//!
//! Split from `agentic.rs` so the report module stays inside the complexity
//! ratchet's budget. It depends on the report types only through the pairs it
//! is handed.

use serde::{Deserialize, Serialize};

use super::super::tune::result::TuneTaskResult;
use super::AgenticTaskComparison;
// Named only by the doc links below.
#[cfg(doc)]
use super::{AgenticEvalReport, EffectVerdict};

/// The paired view of the raw-versus-gglib comparison.
///
/// The two real arms run the **same seeds on the same tasks**, so every
/// `(task, seed)` cell is a matched pair — and pairing is what removes the
/// eval's identical-arm spread from the comparison. The ceiling experiment
/// (tune runs #12–#32, ADR 0004's postscript) resolved a +0.067 effect
/// through noise wider than that *only* because it paired per run; the same
/// data has been sitting in [`AgenticEvalReport::tasks`] all along, compared
/// only as arm means.
///
/// Pairs are on [`TuneTaskResult::tool_match_score`] — the one graded
/// per-run quality scalar. Pass/fail flips remain visible per task in
/// [`AgenticTaskComparison::pass_counts`]; folding them in here would double
/// count, since the match score is most of what decides `passed`.
///
/// The same record serves every pairing the eval makes, each with a baseline
/// and a treatment: raw and gglib here, raw-auto and the proxy arm in
/// [`super::ProxyArms`], and an incumbent and a winner in the tune apply gate.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct PairedEffect {
    /// Matched `(task, seed)` pairs in which both sides produced a real
    /// observation.
    pub pairs: usize,
    /// Pairs both sides ran but at least one side never reached the model —
    /// dropped from every number here, and reported so the drop is visible.
    pub unmeasured_pairs: usize,
    /// Pairs the treatment (gglib, or the proxy arm) scored strictly higher.
    pub wins: usize,
    /// Pairs the baseline (raw, or raw-auto) scored strictly higher.
    pub losses: usize,
    /// Pairs with identical scores. On a suite where most tasks pass cleanly
    /// on both sides this is the largest bucket, and that is information: the
    /// two mostly agree.
    pub ties: usize,
    /// Mean of `treatment − baseline` over the measured pairs.
    pub mean_delta: f64,
    /// One-sided Wilcoxon signed-rank *p* for "the treatment scores higher",
    /// by normal approximation with tie correction.
    ///
    /// `None` below [`WILCOXON_MIN_PAIRS`] non-tied pairs — the approximation
    /// is not trustworthy there, and rendering a statistic the design cannot
    /// support is worse than rendering none (the [`EffectVerdict`] rule). At
    /// small counts, read [`Self::wins`] against [`Self::losses`] instead.
    pub p_value: Option<f64>,
}

/// The fewest non-tied pairs the normal-approximation Wilcoxon accepts.
///
/// Below this the approximation's error is material and an exact table would
/// be needed; above it the correction terms keep it honest.
pub const WILCOXON_MIN_PAIRS: usize = 8;

impl PairedEffect {
    /// Compute the paired comparison from the per-task drill-down.
    ///
    /// `None` when no `(task, seed)` pair has both sides measured — a paired
    /// analysis of nothing is not a zero effect.
    #[must_use]
    pub fn from_tasks(tasks: &[AgenticTaskComparison]) -> Option<Self> {
        Self::from_seed_pairs(
            tasks
                .iter()
                .flat_map(|task| task.raw.iter().zip(task.gglib.iter())),
        )
    }

    /// The paired comparison over matched `(baseline, treatment)` runs of the
    /// same task and seed: the treatment's score minus the baseline's, so
    /// `wins` counts pairs the *treatment* took.
    ///
    /// [`Self::from_tasks`] is this over the raw and gglib arms; the proxy
    /// arm's report pairs its raw-auto baseline with the proxy the same way.
    /// `None` when no pair has both sides measured.
    #[must_use]
    pub fn from_seed_pairs<'a>(
        pairs: impl IntoIterator<Item = (&'a TuneTaskResult, &'a TuneTaskResult)>,
    ) -> Option<Self> {
        let mut deltas = Vec::new();
        let mut unmeasured_pairs = 0_usize;
        for (baseline, treatment) in pairs {
            if baseline.is_measured() && treatment.is_measured() {
                deltas.push(treatment.tool_match_score - baseline.tool_match_score);
            } else {
                unmeasured_pairs += 1;
            }
        }
        Self::from_deltas(&deltas, unmeasured_pairs)
    }

    /// The paired comparison between two runs of the same task list, paired
    /// by `task_id` — the first argument's score minus the second's, so
    /// `wins` counts pairs the *first* run took.
    ///
    /// Built for the tune apply gate (winner versus incumbent), where the
    /// two sides are candidates rather than eval arms. A task present in one
    /// run and absent from the other is skipped, not counted: an unpaired
    /// task has nothing to compare.
    #[must_use]
    pub fn from_paired_runs(a: &[TuneTaskResult], b: &[TuneTaskResult]) -> Option<Self> {
        let b_by_id: std::collections::HashMap<&str, &TuneTaskResult> =
            b.iter().map(|r| (r.task_id.as_str(), r)).collect();
        let mut deltas = Vec::new();
        let mut unmeasured_pairs = 0_usize;
        for left in a {
            let Some(right) = b_by_id.get(left.task_id.as_str()) else {
                continue;
            };
            if left.is_measured() && right.is_measured() {
                deltas.push(left.tool_match_score - right.tool_match_score);
            } else {
                unmeasured_pairs += 1;
            }
        }
        Self::from_deltas(&deltas, unmeasured_pairs)
    }

    /// Aggregate a delta list into the paired record. `None` on no deltas —
    /// a paired analysis of nothing is not a zero effect.
    fn from_deltas(deltas: &[f64], unmeasured_pairs: usize) -> Option<Self> {
        if deltas.is_empty() {
            return None;
        }

        let wins = deltas.iter().filter(|d| **d > 0.0).count();
        let losses = deltas.iter().filter(|d| **d < 0.0).count();
        let ties = deltas.len() - wins - losses;
        #[allow(clippy::cast_precision_loss)]
        let mean_delta = deltas.iter().sum::<f64>() / deltas.len() as f64;

        Some(Self {
            pairs: deltas.len(),
            unmeasured_pairs,
            wins,
            losses,
            ties,
            mean_delta,
            p_value: wilcoxon_one_sided(deltas),
        })
    }
}

/// One-sided Wilcoxon signed-rank *p* for "the deltas are positive".
///
/// Textbook construction: zeros dropped, absolute deltas ranked with average
/// ranks over ties, `W⁻` (the rank sum of the negative deltas) compared
/// against its null distribution by normal approximation with the tie
/// correction and a continuity correction. Small `W⁻` — losses carrying
/// little rank weight — yields small *p*.
///
/// `None` when fewer than [`WILCOXON_MIN_PAIRS`] non-zero deltas remain.
fn wilcoxon_one_sided(deltas: &[f64]) -> Option<f64> {
    let mut nonzero: Vec<f64> = deltas.iter().copied().filter(|d| *d != 0.0).collect();
    let n = nonzero.len();
    if n < WILCOXON_MIN_PAIRS {
        return None;
    }
    nonzero.sort_by(|a, b| a.abs().partial_cmp(&b.abs()).expect("scores are finite"));

    // Average ranks over runs of tied |delta|, accumulating the tie
    // correction term as each run closes.
    let mut w_minus = 0.0_f64;
    let mut tie_correction = 0.0_f64;
    let mut index = 0;
    while index < n {
        let mut end = index + 1;
        // Bitwise equality is the right tie test here: ranks tie when the
        // stored |delta| values are literally the same number, and a margin
        // would invent ties between distinct scores.
        while end < n && (nonzero[end].abs() - nonzero[index].abs()).abs() == 0.0 {
            end += 1;
        }
        #[allow(clippy::cast_precision_loss)]
        let average_rank = ((index + 1 + end) as f64) / 2.0;
        let run = end - index;
        if run > 1 {
            #[allow(clippy::cast_precision_loss)]
            let t = run as f64;
            tie_correction += (t * t).mul_add(t, -t);
        }
        for value in &nonzero[index..end] {
            if *value < 0.0 {
                w_minus += average_rank;
            }
        }
        index = end;
    }

    #[allow(clippy::cast_precision_loss)]
    let nf = n as f64;
    let mean = nf * (nf + 1.0) / 4.0;
    let variance = nf * (nf + 1.0) * 2.0f64.mul_add(nf, 1.0) / 24.0 - tie_correction / 48.0;
    if variance <= 0.0 {
        // Every |delta| identical and tied: the statistic is degenerate, and
        // the sign test the caller can read from wins/losses is the honest
        // fallback.
        return None;
    }
    // Continuity correction toward the mean; "gglib higher" means W⁻ is
    // small, so the one-sided p is the lower tail.
    let z = (w_minus - mean + 0.5) / variance.sqrt();
    Some(normal_cdf(z))
}

/// Standard normal CDF via Abramowitz–Stegun 7.1.26 on `erf`, accurate to
/// ~1.5e-7 — orders of magnitude finer than any decision read from a *p*.
fn normal_cdf(z: f64) -> f64 {
    let x = z / std::f64::consts::SQRT_2;
    let t = 1.0 / 0.327_591_1f64.mul_add(x.abs(), 1.0);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736
                + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let erf = 1.0 - poly * (-x * x).exp();
    let signed = if x < 0.0 { -erf } else { erf };
    0.5 * (1.0 + signed)
}

#[cfg(test)]
#[path = "agentic_paired_tests.rs"]
mod agentic_paired_tests;
