//! The apply gate: whether a tune run's winner may become a model's
//! [`Measured`](crate::domain::DefaultsOrigin::Measured) defaults.
//!
//! The gate exists because a tuner without one optimises noise: it
//! ratchets whichever candidate a lucky draw favoured into the catalog and
//! reports improvement while doing it. Every rule here is the codified form
//! of a failure this repo has already measured — a +0.082 that did not
//! replicate, a control that could not degrade, an arm of 45 zeros that
//! rendered as a score (ADR 0004).
//!
//! ## The in-run calibration pair
//!
//! A tune run has no A/A arm the way the agentic eval does, so the drift
//! estimate is built into the candidate list instead: the **incumbent** — an
//! all-`None` overlay, which resolves through the normal chain and is
//! therefore exactly what the model does today — runs twice. The gap between
//! the twins is the run's own noise, measured under the same tasks, the same
//! server, the same everything. A winner must clear the incumbent's mean by
//! [`EFFECT_NOISE_RATIO`] times that gap before the gate calls it a winner.

use serde::{Deserialize, Serialize};

use super::result::{CandidateSource, TuneCandidateResult};
use crate::domain::benchmark::agentic::{EFFECT_NOISE_RATIO, PairedEffect};

/// What an apply attempt decided, and the numbers it decided on.
///
/// Refusals are first-class outcomes, not errors: each names the evidence
/// that was missing or contrary, because "the gate said no" is only useful
/// if it says what would have changed the answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case")]
pub enum ApplyVerdict {
    /// The winner cleared every gate and may be stored as this model's
    /// measured defaults.
    Apply {
        /// The winning candidate's composite.
        winner_composite: f64,
        /// Mean composite of the incumbent pair.
        incumbent_mean: f64,
        /// `winner_composite − incumbent_mean`.
        margin: f64,
        /// The calibration pair's gap — the run's own drift.
        drift: f64,
        /// The winner-versus-incumbent paired comparison.
        paired: Option<PairedEffect>,
    },
    /// The best candidate *is* the incumbent: the model's current defaults
    /// beat every swept candidate, and there is nothing to apply. Not a
    /// failure — the run answered its question.
    IncumbentStands {
        /// The incumbent pair's mean composite.
        incumbent_mean: f64,
    },
    /// The winner's margin over the incumbent is inside the run's own drift.
    /// Unresolved, not absent — the fix is more tasks or a re-run, never a
    /// smaller threshold.
    WithinDrift {
        /// `winner_composite − incumbent_mean`.
        margin: f64,
        /// The calibration pair's gap.
        drift: f64,
    },
    /// The margin clears the drift, but the per-task paired comparison runs
    /// the other way — the winner's mean rests on a minority of tasks. A
    /// mean and its pairs disagreeing is exactly the shape a lucky outlier
    /// task produces.
    PairedDisagrees {
        /// Pairs the winner took.
        wins: usize,
        /// Pairs the incumbent took.
        losses: usize,
    },
    /// The run carries no incumbent pair, so nothing calibrates it — a run
    /// from before the calibration pair existed, or one whose incumbents
    /// never completed. Nothing can be applied from it.
    Uncalibrated,
    /// The winner or an incumbent has runs that never reached the model, so
    /// their composites are contaminated by a knowable amount and the
    /// comparison is not trustworthy.
    Contaminated {
        /// Unmeasured runs across the compared candidates.
        unmeasured_runs: usize,
    },
}

impl ApplyVerdict {
    /// Whether this verdict licenses writing the winner.
    #[must_use]
    pub const fn applies(&self) -> bool {
        matches!(self, Self::Apply { .. })
    }

    /// Why the gate decided this, in one sentence.
    ///
    /// Split from [`Display`](std::fmt::Display) because the two surfaces want different
    /// amounts: a table column wants the numbers, a detail view wants the
    /// numbers *and* the reasoning. Keeping them apart lets both read from
    /// one source instead of each restating the gate's rules in its own
    /// words — which is how three renderers came to disagree about what a
    /// refusal meant.
    ///
    /// Every sentence here says what would resolve the refusal, because a
    /// gate that only says "no" teaches nobody anything.
    #[must_use]
    pub const fn rationale(&self) -> &'static str {
        match self {
            Self::Apply { .. } => "The margin cleared the run's own drift and the pairs agreed.",
            Self::IncumbentStands { .. } => {
                "No candidate beat the model's current defaults. The run answered its \
                 question, and the answer is 'change nothing'."
            }
            Self::WithinDrift { .. } => {
                "The winner's margin is inside the run's own drift. Unresolved, not \
                 absent; more tasks or a re-run resolves it, a smaller threshold never \
                 does."
            }
            Self::PairedDisagrees { .. } => {
                "The winner's mean rests on a minority of tasks — the lucky-outlier \
                 shape, refused by the pairs."
            }
            Self::Uncalibrated => {
                "This run has no incumbent calibration pair, so nothing measures its \
                 drift. Re-run the tune; every new run carries the pair."
            }
            Self::Contaminated { .. } => {
                "Some task runs never reached the model, so the compared scores are \
                 contaminated. A zero from a dead upstream is not a low score."
            }
        }
    }
}

/// The verdict's headline: what happened, with the numbers that decided it.
///
/// Deliberately unstyled and single-line so every surface can wrap it in its
/// own presentation. Pair with [`ApplyVerdict::rationale`] where there is
/// room to explain.
impl std::fmt::Display for ApplyVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Apply {
                winner_composite,
                incumbent_mean,
                margin,
                drift,
                ..
            } => write!(
                f,
                "applied: winner {winner_composite:.3} over incumbent \
                 {incumbent_mean:.3}, margin {margin:+.3} against drift {drift:.3}"
            ),
            Self::IncumbentStands { incumbent_mean } => {
                write!(f, "refused: incumbent stands at {incumbent_mean:.3}")
            }
            Self::WithinDrift { margin, drift } => {
                write!(f, "refused: margin {margin:+.3} within drift {drift:.3}")
            }
            Self::PairedDisagrees { wins, losses } => {
                write!(f, "refused: pairs disagree ({wins}W-{losses}L)")
            }
            Self::Uncalibrated => write!(f, "refused: uncalibrated run"),
            Self::Contaminated { unmeasured_runs } => {
                write!(f, "refused: {unmeasured_runs} unmeasured run(s)")
            }
        }
    }
}

/// Evaluate a completed tune run's candidates against the apply gate.
///
/// `candidates` is the run's full stored list. The winner is the highest
/// composite among full-suite, measured, non-calibration candidates; the
/// incumbent pair is found by [`CandidateSource`].
#[must_use]
pub fn evaluate_apply(candidates: &[TuneCandidateResult]) -> ApplyVerdict {
    let incumbents: Vec<&TuneCandidateResult> = candidates
        .iter()
        .filter(|c| {
            matches!(
                c.source,
                CandidateSource::Incumbent | CandidateSource::IncumbentCalibration
            ) && !c.pruned
        })
        .collect();
    let [first, second] = incumbents.as_slice() else {
        return ApplyVerdict::Uncalibrated;
    };

    let Some(winner) = candidates
        .iter()
        .filter(|c| {
            !c.pruned
                && !matches!(
                    c.source,
                    CandidateSource::Incumbent | CandidateSource::IncumbentCalibration
                )
        })
        .max_by(|a, b| {
            a.composite_score
                .partial_cmp(&b.composite_score)
                .expect("composites are finite")
        })
    else {
        // Only the incumbent pair survived: the sweep produced nothing to
        // compare, which is the incumbent standing by default.
        return ApplyVerdict::IncumbentStands {
            incumbent_mean: f64::midpoint(first.composite_score, second.composite_score),
        };
    };

    let unmeasured_runs = unmeasured(winner) + unmeasured(first) + unmeasured(second);
    if unmeasured_runs > 0 {
        return ApplyVerdict::Contaminated { unmeasured_runs };
    }

    let incumbent_mean = f64::midpoint(first.composite_score, second.composite_score);
    let drift = (first.composite_score - second.composite_score).abs();
    let margin = winner.composite_score - incumbent_mean;

    if margin <= 0.0 {
        return ApplyVerdict::IncumbentStands { incumbent_mean };
    }
    if margin < EFFECT_NOISE_RATIO * drift {
        return ApplyVerdict::WithinDrift { margin, drift };
    }

    // Direction check: the winner's mean must not rest on a minority of
    // tasks. Compared against the first incumbent twin — either would do,
    // and mixing both would double the incumbent's task list against the
    // winner's single one.
    let paired = PairedEffect::from_paired_runs(&winner.task_results, &first.task_results);
    if let Some(p) = &paired
        && p.losses > p.wins
    {
        return ApplyVerdict::PairedDisagrees {
            wins: p.wins,
            losses: p.losses,
        };
    }

    ApplyVerdict::Apply {
        winner_composite: winner.composite_score,
        incumbent_mean,
        margin,
        drift,
        paired,
    }
}

/// The winner a verdict of [`ApplyVerdict::Apply`] refers to.
///
/// Re-derived by the same rule `evaluate_apply` uses, so the applier and the
/// gate cannot disagree about which candidate won.
#[must_use]
pub fn winning_candidate(candidates: &[TuneCandidateResult]) -> Option<&TuneCandidateResult> {
    candidates
        .iter()
        .filter(|c| {
            !c.pruned
                && !matches!(
                    c.source,
                    CandidateSource::Incumbent | CandidateSource::IncumbentCalibration
                )
        })
        .max_by(|a, b| {
            a.composite_score
                .partial_cmp(&b.composite_score)
                .expect("composites are finite")
        })
}

fn unmeasured(candidate: &TuneCandidateResult) -> usize {
    candidate
        .task_results
        .iter()
        .filter(|r| !r.is_measured())
        .count()
}

/// The durable record of an apply, stored on the run row so
/// `gglib model explain`'s "measured by a tune sweep" can be traced to the
/// numbers that licensed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApplyRecord {
    /// The verdict that licensed the write, with its numbers.
    pub verdict: ApplyVerdict,
    /// The applied sampling overlay, exactly as stored on the model.
    /// `None` on a refusal record — the verdict says why nothing was
    /// written. (Optional since refusals began leaving records; an apply
    /// written before that always carries `Some`.)
    #[serde(default)]
    pub applied_config: Option<crate::domain::InferenceConfig>,
    /// The defaults the apply displaced, exactly as they were stored.
    ///
    /// What makes an apply reversible without archaeology: a signal-driven
    /// sweep that made things worse can be undone from the run row alone.
    /// `None` on records written before the field existed, and a real
    /// `Some(None)`-shaped absence is representable — a model that had no
    /// stored defaults restores to having none.
    #[serde(default)]
    pub prior_defaults: Option<Option<crate::domain::InferenceConfig>>,
    /// The origin the displaced defaults carried.
    #[serde(default)]
    pub prior_origin: Option<Option<crate::domain::DefaultsOrigin>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::InferenceConfig;
    use crate::domain::benchmark::tune::result::TuneTaskResult;
    use crate::domain::benchmark::tune::task::TaskCategory;

    fn task(id: &str, score: f64) -> TuneTaskResult {
        TuneTaskResult {
            task_id: id.to_owned(),
            category: TaskCategory::SingleCall,
            passed: score >= 1.0,
            tool_match_score: score,
            loop_detected: false,
            stagnation_detected: false,
            iterations: 1,
            latency_ms: 10,
            completion_tokens: Some(100),
            time_to_first_tool_call_ms: Some(5),
            detail: None,
            unmeasured: None,
        }
    }

    fn candidate(source: CandidateSource, scores: &[f64]) -> TuneCandidateResult {
        #[allow(clippy::cast_precision_loss)]
        let composite = scores.iter().sum::<f64>() / scores.len() as f64;
        TuneCandidateResult {
            config: InferenceConfig::default(),
            source,
            task_results: scores
                .iter()
                .enumerate()
                .map(|(i, s)| task(&format!("t{i}"), *s))
                .collect(),
            composite_score: composite,
            pruned: false,
            tg_tps: None,
        }
    }

    fn incumbent_pair(scores: &[f64], twin_scores: &[f64]) -> Vec<TuneCandidateResult> {
        vec![
            candidate(CandidateSource::Incumbent, scores),
            candidate(CandidateSource::IncumbentCalibration, twin_scores),
        ]
    }

    /// The headline path: a winner clearly above the incumbent pair, with
    /// the pairs agreeing, applies.
    #[test]
    fn a_clear_winner_applies() {
        let mut candidates = incumbent_pair(&[0.5, 0.5, 0.5], &[0.52, 0.5, 0.5]);
        candidates.push(candidate(CandidateSource::UserGrid, &[0.9, 0.9, 0.9]));
        let verdict = evaluate_apply(&candidates);
        assert!(verdict.applies(), "{verdict:?}");
    }

    /// The gate's core rule: a margin inside the run's own drift is
    /// unresolved, and unresolved never applies.
    #[test]
    fn a_margin_within_drift_is_refused() {
        // Incumbent twins 0.3 apart: the run is very noisy.
        let mut candidates = incumbent_pair(&[0.5, 0.5, 0.6], &[0.8, 0.8, 0.9]);
        // The winner beats the incumbent mean by less than 2× that gap.
        candidates.push(candidate(CandidateSource::UserGrid, &[0.9, 0.8, 0.9]));
        match evaluate_apply(&candidates) {
            ApplyVerdict::WithinDrift { margin, drift } => {
                assert!(margin < EFFECT_NOISE_RATIO * drift, "{margin} vs {drift}");
            }
            other => panic!("expected WithinDrift, got {other:?}"),
        }
    }

    /// A run without the calibration pair — every run recorded before the
    /// pair existed — cannot be applied from, whatever its scores say.
    #[test]
    fn a_run_without_the_incumbent_pair_is_uncalibrated() {
        let candidates = vec![candidate(CandidateSource::UserGrid, &[1.0, 1.0, 1.0])];
        assert_eq!(evaluate_apply(&candidates), ApplyVerdict::Uncalibrated);
    }

    /// A winner that does not beat the incumbent is the incumbent standing —
    /// a successful run whose answer is "change nothing".
    #[test]
    fn an_unbeaten_incumbent_stands() {
        let mut candidates = incumbent_pair(&[0.9, 0.9, 0.9], &[0.9, 0.9, 0.9]);
        candidates.push(candidate(CandidateSource::UserGrid, &[0.5, 0.5, 0.5]));
        assert!(matches!(
            evaluate_apply(&candidates),
            ApplyVerdict::IncumbentStands { .. }
        ));
    }

    /// Unmeasured runs in either side of the comparison poison it: a zero
    /// from a dead upstream is not a low score (the 45-zeros lesson,
    /// ADR 0004).
    #[test]
    fn contaminated_candidates_are_refused() {
        let mut candidates = incumbent_pair(&[0.5, 0.5, 0.5], &[0.5, 0.5, 0.5]);
        let mut winner = candidate(CandidateSource::UserGrid, &[0.9, 0.9, 0.9]);
        winner.task_results[1].unmeasured = Some("upstream died".to_owned());
        candidates.push(winner);
        assert!(matches!(
            evaluate_apply(&candidates),
            ApplyVerdict::Contaminated { unmeasured_runs: 1 }
        ));
    }

    /// A mean carried by one outlier task while the incumbent wins the rest
    /// is refused: the pairs outvote the mean.
    #[test]
    fn a_minority_winner_is_refused_by_the_pairs() {
        // The winner's mean (0.55) clears the incumbent mean (0.5) and the
        // twins' zero drift — but it rests entirely on one outlier task
        // while the incumbent takes the other three.
        let mut candidates = incumbent_pair(&[0.5, 0.5, 0.5, 0.5], &[0.5, 0.5, 0.5, 0.5]);
        candidates.push(candidate(CandidateSource::UserGrid, &[1.0, 0.4, 0.4, 0.4]));
        match evaluate_apply(&candidates) {
            ApplyVerdict::PairedDisagrees { wins, losses } => {
                assert_eq!((wins, losses), (1, 3));
            }
            other => panic!("expected PairedDisagrees, got {other:?}"),
        }
    }

    /// A pruned candidate never wins: its composite covers only the
    /// pre-screen tasks and is not comparable with a full-suite score.
    #[test]
    fn a_pruned_candidate_cannot_win() {
        let mut candidates = incumbent_pair(&[0.5, 0.5, 0.5], &[0.5, 0.5, 0.5]);
        let mut pruned = candidate(CandidateSource::UserGrid, &[1.0, 1.0, 1.0]);
        pruned.pruned = true;
        candidates.push(pruned);
        assert!(matches!(
            evaluate_apply(&candidates),
            ApplyVerdict::IncumbentStands { .. }
        ));
    }
}
