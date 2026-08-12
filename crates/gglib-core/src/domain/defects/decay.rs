//! What restored defect evidence is still worth, and what should be written
//! back.
//!
//! Entirely pure: every function here is a total function of its arguments,
//! with no clock, no environment and no I/O. The caller supplies `now`, the
//! half-life and the build string; this module only decides. That is what
//! makes the two staleness answers — decay by age, discard by build —
//! testable without a database or a running daemon, which is the whole
//! reason they live here rather than beside the code that persists them.

use std::collections::HashMap;
use std::hash::BuildHasher;

use chrono::{DateTime, Utc};

use super::{ModelDefectCounts, PersistedDefectWindow, delta};

/// The exponential decay a gap of `gap_secs` applies: `0.5^(gap/half_life)`.
///
/// A non-positive gap — clock skew, or a stamp from the future — decays
/// nothing. Evidence is at worst current; it is never *amplified* by a clock
/// that disagrees with itself.
#[must_use]
pub fn decay_factor(gap_secs: f64, half_life_secs: f64) -> f64 {
    if gap_secs <= 0.0 {
        return 1.0;
    }
    0.5_f64.powf(gap_secs / half_life_secs)
}

/// Each count scaled and rounded independently.
///
/// A window is a bundle of counts, not a ratio, so per-field rounding noise
/// of ±0.5 events is accepted. Documented here so nobody "fixes" it into a
/// scheme that preserves ratios by inventing fractional events — a rate is
/// the reader's job to compute, and it would rather divide two honest
/// integers than two doctored ones.
#[must_use]
pub fn decay_counts(counts: ModelDefectCounts, factor: f64) -> ModelDefectCounts {
    let scale = |c: u64| -> u64 {
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        {
            ((c as f64) * factor).round() as u64
        }
    };
    ModelDefectCounts {
        requests: scale(counts.requests),
        loop_guard_trips: scale(counts.loop_guard_trips),
        repairs_attempted: scale(counts.repairs_attempted),
        repairs_succeeded: scale(counts.repairs_succeeded),
        stream_errors: scale(counts.stream_errors),
    }
}

/// What boot does with the persisted rows: seed these, delete those.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RestorePlan {
    /// Rows to seed into the ledger, already decayed.
    pub seed: Vec<(String, ModelDefectCounts)>,
    /// Rows to delete: foreign-build evidence, and windows decayed to
    /// nothing.
    pub discard: Vec<String>,
}

/// Sort persisted rows into seeds and discards.
///
/// A row from another llama.cpp release is discarded outright — another
/// build's rate must not answer this build's question at *any* weight, so
/// this is a rejection rather than a heavier decay. A surviving row decays
/// by its age; one whose decayed request count rounds below a single request
/// has nothing left to say and is deleted rather than carried forever.
#[must_use]
pub fn restore_plan(
    rows: Vec<PersistedDefectWindow>,
    now: DateTime<Utc>,
    llama_build: &str,
    half_life_secs: f64,
) -> RestorePlan {
    let mut plan = RestorePlan::default();
    for row in rows {
        if row.llama_build != llama_build {
            plan.discard.push(row.model_name);
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let gap_secs = (now - row.updated_at).num_seconds() as f64;
        let decayed = decay_counts(row.counts, decay_factor(gap_secs, half_life_secs));
        if decayed.requests == 0 {
            plan.discard.push(row.model_name);
            continue;
        }
        plan.seed.push((row.model_name, decayed));
    }
    plan
}

/// The per-model window: current counts minus the reader's baseline.
#[must_use]
pub fn windowed<C: BuildHasher, B: BuildHasher>(
    current: &HashMap<String, ModelDefectCounts, C>,
    baselines: &HashMap<String, ModelDefectCounts, B>,
) -> HashMap<String, ModelDefectCounts> {
    current
        .iter()
        .map(|(name, counts)| {
            let baseline = baselines.get(name).copied().unwrap_or_default();
            (name.clone(), delta(*counts, baseline))
        })
        .collect()
}

/// The rows one flush writes: every window that differs from what was last
/// persisted, stamped with `now` and the current release.
///
/// Skipping unchanged rows is not just economy — it preserves a stored row's
/// `updated_at`, so evidence that sits untouched keeps decaying from its real
/// age instead of being re-dated by every flush. Re-stamping an idle window
/// would make it immortal.
#[must_use]
pub fn rows_to_flush<W: BuildHasher, L: BuildHasher>(
    windows: &HashMap<String, ModelDefectCounts, W>,
    last_flushed: &HashMap<String, ModelDefectCounts, L>,
    now: DateTime<Utc>,
    llama_build: &str,
) -> Vec<PersistedDefectWindow> {
    windows
        .iter()
        .filter(|(name, counts)| last_flushed.get(*name) != Some(counts))
        .map(|(name, counts)| PersistedDefectWindow {
            model_name: name.clone(),
            counts: *counts,
            updated_at: now,
            llama_build: llama_build.to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF_LIFE: f64 = 86_400.0; // one day

    fn counts(requests: u64) -> ModelDefectCounts {
        ModelDefectCounts {
            requests,
            ..ModelDefectCounts::default()
        }
    }

    fn row(name: &str, requests: u64, age_secs: i64, build: &str) -> PersistedDefectWindow {
        PersistedDefectWindow {
            model_name: name.to_owned(),
            counts: counts(requests),
            updated_at: Utc::now() - chrono::Duration::seconds(age_secs),
            llama_build: build.to_owned(),
        }
    }

    #[test]
    fn one_half_life_halves_the_evidence() {
        assert!((decay_factor(HALF_LIFE, HALF_LIFE) - 0.5).abs() < f64::EPSILON);
        assert!((decay_factor(2.0 * HALF_LIFE, HALF_LIFE) - 0.25).abs() < f64::EPSILON);
    }

    /// Clock skew must never make evidence stronger than when it was written.
    #[test]
    fn a_gap_that_runs_backwards_decays_nothing() {
        assert!((decay_factor(-500.0, HALF_LIFE) - 1.0).abs() < f64::EPSILON);
        assert!((decay_factor(0.0, HALF_LIFE) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn every_field_decays_including_stream_errors() {
        let full = ModelDefectCounts {
            requests: 100,
            loop_guard_trips: 10,
            repairs_attempted: 8,
            repairs_succeeded: 4,
            stream_errors: 6,
        };
        let halved = decay_counts(full, 0.5);
        assert_eq!(halved.requests, 50);
        assert_eq!(halved.loop_guard_trips, 5);
        assert_eq!(halved.repairs_attempted, 4);
        assert_eq!(halved.repairs_succeeded, 2);
        assert_eq!(halved.stream_errors, 3);
    }

    /// A foreign build is rejected outright, not merely discounted — the
    /// distinction the whole build-scoping argument rests on.
    #[test]
    fn another_builds_evidence_is_discarded_however_fresh() {
        let plan = restore_plan(
            vec![row("m", 10_000, 0, "b10000")],
            Utc::now(),
            "b10327",
            HALF_LIFE,
        );
        assert!(plan.seed.is_empty(), "no weight at all, not reduced weight");
        assert_eq!(plan.discard, vec!["m".to_owned()]);
    }

    #[test]
    fn fresh_same_build_evidence_survives_intact() {
        let plan = restore_plan(
            vec![row("m", 40, 0, "b10327")],
            Utc::now(),
            "b10327",
            HALF_LIFE,
        );
        assert_eq!(plan.seed, vec![("m".to_owned(), counts(40))]);
        assert!(plan.discard.is_empty());
    }

    /// Evidence too old to round to a single request is deleted rather than
    /// carried forever as a zero row.
    #[test]
    fn a_window_decayed_below_one_request_is_discarded() {
        let plan = restore_plan(
            vec![row("m", 4, 20 * 86_400, "b10327")],
            Utc::now(),
            "b10327",
            HALF_LIFE,
        );
        assert!(plan.seed.is_empty());
        assert_eq!(plan.discard, vec!["m".to_owned()]);
    }

    #[test]
    fn windowing_subtracts_the_baseline_and_treats_absent_as_zero() {
        let current = HashMap::from([("a".to_owned(), counts(10)), ("b".to_owned(), counts(3))]);
        let baselines = HashMap::from([("a".to_owned(), counts(4))]);

        let w = windowed(&current, &baselines);
        assert_eq!(w["a"].requests, 6);
        assert_eq!(w["b"].requests, 3, "no baseline means the whole count");
    }

    /// The immortality guard: an unchanged window is not rewritten, so its
    /// stored `updated_at` keeps ageing and decay keeps biting.
    #[test]
    fn an_unchanged_window_is_not_reflushed() {
        let windows = HashMap::from([("a".to_owned(), counts(5)), ("b".to_owned(), counts(9))]);
        let last = HashMap::from([("a".to_owned(), counts(5))]);

        let rows = rows_to_flush(&windows, &last, Utc::now(), "b10327");
        assert_eq!(rows.len(), 1, "only the changed window");
        assert_eq!(rows[0].model_name, "b");
    }

    #[test]
    fn flushed_rows_carry_the_current_build_and_stamp() {
        let now = Utc::now();
        let windows = HashMap::from([("a".to_owned(), counts(5))]);

        let rows = rows_to_flush(&windows, &HashMap::new(), now, "b10327");
        assert_eq!(rows[0].llama_build, "b10327");
        assert_eq!(rows[0].updated_at, now);
    }

    /// Round-trip: what a flush writes is what a same-build restore reads
    /// back, undecayed when no time has passed.
    #[test]
    fn a_flush_round_trips_through_restore_unchanged() {
        let now = Utc::now();
        let windows = HashMap::from([("a".to_owned(), counts(7))]);
        let rows = rows_to_flush(&windows, &HashMap::new(), now, "b10327");

        let plan = restore_plan(rows, now, "b10327", HALF_LIFE);
        assert_eq!(plan.seed, vec![("a".to_owned(), counts(7))]);
    }
}
