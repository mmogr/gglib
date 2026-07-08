//! Successive-halving candidate reduction for the pre-screen round.
//!
//! Running every candidate through the full task suite is wasteful when a
//! candidate is obviously bad — the pre-screen round evaluates every
//! candidate against a cheap subset of tasks first, and this module decides
//! which candidates survive to run the full suite.

use std::cmp::Ordering;

/// Select the indices (into `scores`) of candidates that survive the
/// pre-screen round, ranked by score descending.
///
/// Keeps `ceil(n * (1 - prune_fraction))` candidates, with a floor of `3`
/// survivors (or all candidates, if fewer than `3` exist) so a single
/// unlucky pre-screen task never collapses the search space too
/// aggressively. `prune_fraction` is clamped to `[0.0, 0.9]` — pruning
/// everything but one candidate would defeat the purpose of a sweep.
///
/// Returned indices are sorted ascending (original order), not by rank.
#[must_use]
pub fn select_survivors(scores: &[f64], prune_fraction: f32) -> Vec<usize> {
    let n = scores.len();
    if n == 0 {
        return Vec::new();
    }

    let prune_fraction = prune_fraction.clamp(0.0, 0.9);
    #[allow(clippy::cast_precision_loss)]
    let n_f32 = n as f32;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let target_keep = (n_f32 * (1.0 - prune_fraction)).ceil() as usize;
    let keep_count = target_keep.max(3).min(n);

    let mut ranked: Vec<usize> = (0..n).collect();
    ranked.sort_by(|&a, &b| scores[b].partial_cmp(&scores[a]).unwrap_or(Ordering::Equal));
    ranked.truncate(keep_count);
    ranked.sort_unstable();
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scores_returns_empty() {
        assert_eq!(select_survivors(&[], 0.5), Vec::<usize>::new());
    }

    #[test]
    fn keeps_top_half_by_score() {
        let scores = vec![0.9, 0.1, 0.8, 0.2, 0.7, 0.3, 0.6, 0.4];
        let survivors = select_survivors(&scores, 0.5);
        // 8 candidates, prune 50% -> keep 4, the highest-scoring ones.
        assert_eq!(survivors.len(), 4);
        for &i in &survivors {
            assert!(scores[i] >= 0.6, "expected top-4 score, got {}", scores[i]);
        }
    }

    #[test]
    fn floor_of_three_survivors_even_with_aggressive_pruning() {
        let scores = vec![0.9, 0.5, 0.4, 0.1];
        let survivors = select_survivors(&scores, 0.9);
        assert_eq!(survivors.len(), 3);
    }

    #[test]
    fn never_keeps_more_than_available_when_fewer_than_floor() {
        let scores = vec![0.9, 0.5];
        let survivors = select_survivors(&scores, 0.0);
        assert_eq!(survivors.len(), 2);
    }

    #[test]
    fn prune_fraction_is_clamped_to_valid_range() {
        // A prune_fraction of 1.0 (clamped to 0.9) must never drop to zero
        // survivors — the floor of 3 (or all, if fewer) always applies.
        let scores = vec![0.9, 0.8, 0.7, 0.6, 0.5];
        let survivors = select_survivors(&scores, 1.0);
        assert_eq!(survivors.len(), 3);
    }

    #[test]
    fn returned_indices_are_sorted_ascending() {
        let scores = vec![0.1, 0.9, 0.2, 0.8, 0.3];
        let survivors = select_survivors(&scores, 0.5);
        let mut sorted = survivors.clone();
        sorted.sort_unstable();
        assert_eq!(survivors, sorted);
    }
}
