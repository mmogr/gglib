//! Each figure from known inputs, and each in the words the GUI says it in
//! (`readings/turnFigures.ts`, `readings/rate.ts`).

use super::{count_words, rate_words, seconds_words, tokens_per_second, tokens_words};

#[test]
fn a_rate_is_tokens_over_the_seconds_writing() {
    assert_eq!(
        tokens_per_second(Some(496), Some(38_200)),
        Some(496.0 / 38.2)
    );
    assert_eq!(tokens_per_second(Some(2), Some(500)), Some(4.0));
}

/// No rate is ever a guess: without either figure, with no time between the
/// first token and the last, or with fewer than two tokens there is none.
#[test]
fn there_is_no_rate_without_both_figures_or_with_no_time_writing() {
    assert_eq!(tokens_per_second(None, Some(1000)), None);
    assert_eq!(tokens_per_second(Some(40), None), None);
    assert_eq!(tokens_per_second(Some(40), Some(0)), None);
    assert_eq!(tokens_per_second(Some(0), Some(1000)), None);
    assert_eq!(tokens_per_second(Some(1), Some(1000)), None);
}

/// The cases are `toFixed(1)`'s, which the GUI's `rateWords` uses:
/// the nearest of the stored value (1.15 is stored a little under, 2.45 a
/// little over), and an exact tie up.
#[test]
fn a_rate_keeps_one_decimal_under_ten_and_is_whole_above() {
    assert_eq!(rate_words(34.4), "34 t/s");
    assert_eq!(rate_words(12.98), "13 t/s");
    assert_eq!(rate_words(8.5), "8.5 t/s");
    assert_eq!(rate_words(8.25), "8.3 t/s");
    assert_eq!(rate_words(8.75), "8.8 t/s");
    assert_eq!(rate_words(1.15), "1.1 t/s");
    assert_eq!(rate_words(9.95), "9.9 t/s");
    assert_eq!(rate_words(2.45), "2.5 t/s");
    assert_eq!(rate_words(4.0), "4.0 t/s");
    assert_eq!(rate_words(10.0), "10 t/s");
}

/// The cases `secondsWords` is tested with in the GUI.
#[test]
fn seconds_are_said_as_a_turn_says_them() {
    assert_eq!(seconds_words(0.4), "0.4 s");
    assert_eq!(seconds_words(4.7), "4.7 s");
    assert_eq!(seconds_words(3.0), "3 s");
    assert_eq!(seconds_words(19.2), "19 s");
    assert_eq!(seconds_words(83.0), "1 min 23 s");
    assert_eq!(seconds_words(120.0), "2 min");
    assert_eq!(seconds_words(9.96), "10 s");
    assert_eq!(seconds_words(59.6), "1 min");
    assert_eq!(seconds_words(41.0), "41 s");
}

#[test]
fn a_count_has_commas_between_its_thousands() {
    assert_eq!(count_words(0), "0");
    assert_eq!(count_words(999), "999");
    assert_eq!(count_words(2_100), "2,100");
    assert_eq!(count_words(1_234_567), "1,234,567");
}

#[test]
fn one_token_is_a_token() {
    assert_eq!(tokens_words(0), "0 tokens");
    assert_eq!(tokens_words(1), "1 token");
    assert_eq!(tokens_words(2), "2 tokens");
    assert_eq!(tokens_words(2_100), "2,100 tokens");
}
