//! Tests for the paired statistic in `agentic_paired.rs`.

use super::super::agentic_tests::paired_task;
use super::*;

#[test]
fn paired_effect_counts_wins_losses_ties_and_means_the_deltas() {
    let tasks = vec![
        paired_task("a", &[0.5, 1.0], &[1.0, 1.0]), // one win, one tie
        paired_task("b", &[1.0], &[0.5]),           // one loss
    ];
    let paired = PairedEffect::from_tasks(&tasks).expect("pairs exist");
    assert_eq!(paired.pairs, 3);
    assert_eq!((paired.wins, paired.losses, paired.ties), (1, 1, 1));
    assert!((paired.mean_delta - 0.0).abs() < 1e-12, "{paired:?}");
    assert_eq!(paired.unmeasured_pairs, 0);
    // Two non-tied pairs is far below the Wilcoxon minimum.
    assert_eq!(paired.p_value, None);
}

/// A pair either side of which never reached the model is dropped and
/// counted, never scored — the same rule `ArmScores::unmeasured_runs`
/// applies arm-wide, kept at pair granularity here.
#[test]
fn paired_effect_drops_unmeasured_pairs_and_says_so() {
    let mut task = paired_task("a", &[0.5, 0.5], &[1.0, 1.0]);
    task.raw[1].unmeasured = Some("upstream unreachable".to_owned());
    let paired = PairedEffect::from_tasks(&[task]).expect("one live pair");
    assert_eq!(paired.pairs, 1);
    assert_eq!(paired.unmeasured_pairs, 1);
    assert_eq!(paired.wins, 1);
}

#[test]
fn paired_effect_is_none_when_nothing_was_measured() {
    assert!(PairedEffect::from_tasks(&[]).is_none());
    let mut task = paired_task("a", &[0.5], &[1.0]);
    task.gglib[0].unmeasured = Some("dead".to_owned());
    assert!(PairedEffect::from_tasks(&[task]).is_none());
}

/// Ten distinct all-positive deltas: W⁻ = 0, and the normal approximation
/// with continuity correction gives z = (0 − 27.5 + 0.5)/√96.25 ≈ −2.752,
/// p ≈ 0.0030. Pinned inside a band an implementation error of one rank,
/// one correction term, or a dropped tail would leave.
#[test]
fn wilcoxon_all_positive_deltas_is_a_strong_result() {
    let gglib: Vec<f64> = (1..=10).map(|i| f64::from(i) * 0.05).collect();
    let raw = vec![0.0; 10];
    let tasks = vec![paired_task("a", &raw, &gglib)];
    let p = PairedEffect::from_tasks(&tasks)
        .unwrap()
        .p_value
        .expect("ten non-tied pairs");
    assert!(p > 0.001 && p < 0.005, "p = {p}");
}

/// Symmetric wins and losses of matching magnitude: W⁻ lands on its null
/// mean and the one-sided p sits at chance.
#[test]
fn wilcoxon_balanced_deltas_read_as_chance() {
    let raw = vec![0.5; 10];
    let gglib = vec![0.6, 0.4, 0.7, 0.3, 0.8, 0.2, 0.9, 0.1, 1.0, 0.0];
    let tasks = vec![paired_task("a", &raw, &gglib)];
    let p = PairedEffect::from_tasks(&tasks)
        .unwrap()
        .p_value
        .expect("ten non-tied pairs");
    assert!(p > 0.4 && p < 0.6, "p = {p}");
}

/// Below the minimum the statistic says nothing — ties do not count
/// toward the minimum, because zeros are dropped before ranking.
#[test]
fn wilcoxon_says_nothing_below_the_minimum() {
    let raw = vec![0.5; 10];
    let mut gglib = vec![0.5; 10]; // ties everywhere...
    for (i, value) in gglib.iter_mut().enumerate().take(7) {
        *value = 0.01f64.mul_add(f64::from(u8::try_from(i).unwrap()), 0.6);
    }
    let tasks = vec![paired_task("a", &raw, &gglib)];
    let paired = PairedEffect::from_tasks(&tasks).unwrap();
    assert_eq!(paired.pairs, 10);
    assert_eq!(paired.wins, 7);
    assert_eq!(paired.p_value, None);
}
