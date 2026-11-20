//! The figures every surface says the same way: how fast a model wrote, how
//! long something took, a count, and what a request's images cost.
//!
//! A writing rate is computed here once, for a proxied request
//! (`ContextSnapshot::tokens_per_second`) and for an agent turn
//! ([`TurnUsage::tokens_per_second`](super::agent::TurnUsage::tokens_per_second)),
//! and said here once, in the words `gglib proxy dashboard` and `gglib chat`
//! print. The GUI is served the number and says it the same way; the words
//! are pinned on both sides by `contracts/runs/turn_made.json` and
//! `contracts/proxy/figures.json`.

use std::fmt::Write as _;
use std::time::Duration;

/// Tokens written per second of writing: `tokens` over `ms`.
///
/// `None` without both figures, with no time between the first token and
/// the last, and for fewer than two tokens, which have no time writing.
#[must_use]
pub fn tokens_per_second(tokens: Option<u32>, ms: Option<u64>) -> Option<f64> {
    let (tokens, ms) = (tokens?, ms?);
    if tokens < 2 || ms == 0 {
        return None;
    }
    let seconds = Duration::from_millis(ms).as_secs_f64();
    Some(f64::from(tokens) / seconds)
}

/// A rate as every surface says it: "34 t/s", "8.5 t/s". Under ten keeps one
/// decimal, else it is rounded whole, a half up, as the GUI's `rateWords`
/// (`readings/rate.ts`) rounds it.
#[must_use]
pub fn rate_words(rate: f64) -> String {
    if rate < 10.0 {
        format!("{} t/s", one_decimal(rate))
    } else {
        format!("{} t/s", rate.round())
    }
}

/// `value` to one decimal as JavaScript's `toFixed(1)` writes it: the
/// nearest of the stored value, as `{:.1}` finds it too, but a tie (a value
/// stored exactly halfway between two tenths, which in binary is only ever
/// an odd quarter: 8.25, 8.75) goes up where `{:.1}` would go to the even
/// digit.
fn one_decimal(value: f64) -> String {
    let quarters = value * 4.0;
    if quarters.fract() == 0.0 && (quarters / 2.0).fract() != 0.0 {
        format!("{:.1}", (value * 10.0).ceil() / 10.0)
    } else {
        format!("{value:.1}")
    }
}

/// Seconds as a turn says them: "0.4 s", "4.7 s" and "3 s" under ten, "19 s",
/// "1 min 23 s", "2 min".
///
/// Rounded to the precision shown before the unit is
/// chosen, so 9.96 is "10 s" and 59.6 is "1 min", never "10.0 s" or "60 s";
/// a whole number drops its ".0", and a whole minute its "0 s".
#[must_use]
pub fn seconds_words(seconds: f64) -> String {
    let tenths = (seconds * 10.0).round() / 10.0;
    if tenths < 10.0 {
        return if tenths.fract() == 0.0 {
            format!("{tenths:.0} s")
        } else {
            format!("{tenths:.1} s")
        };
    }
    let whole = seconds.round();
    if whole < 60.0 {
        return format!("{whole:.0} s");
    }
    let minutes = (whole / 60.0).floor();
    let rest = whole - minutes * 60.0;
    if rest == 0.0 {
        format!("{minutes:.0} min")
    } else {
        format!("{minutes:.0} min {rest:.0} s")
    }
}

/// A count of tokens as every surface says it: "1 token", "2,100 tokens".
#[must_use]
pub fn tokens_words(count: u64) -> String {
    let unit = if count == 1 { "token" } else { "tokens" };
    format!("{} {unit}", count_words(count))
}

/// A count with `,` between its thousands: "2,100".
#[must_use]
pub fn count_words(count: u64) -> String {
    let digits = count.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// What a request's images cost, as every surface says it; `None` for a
/// request with no image.
///
/// First how many there are and the prompt tokens they were estimated at,
/// "1 image ~3,600 tokens". Then, once the model's server has counted it, the
/// whole prompt, text and images together: "2 images ~7,200 tokens · 7,451
/// prompt tokens".
///
/// The proxy serves these words on each request's record
/// (`ContextSnapshot::image_words`), and `gglib proxy dashboard` and the GUI
/// print them as served.
#[must_use]
pub fn image_cost_words(
    images: usize,
    image_tokens: usize,
    prompt_tokens: Option<u32>,
) -> Option<String> {
    if images == 0 {
        return None;
    }
    let wide = |count: usize| u64::try_from(count).unwrap_or(u64::MAX);
    let unit = if images == 1 { "image" } else { "images" };
    let mut words = format!(
        "{} {unit} ~{}",
        count_words(wide(images)),
        tokens_words(wide(image_tokens))
    );
    if let Some(prompt) = prompt_tokens {
        let unit = if prompt == 1 { "token" } else { "tokens" };
        let _ = write!(
            words,
            " \u{b7} {} prompt {unit}",
            count_words(u64::from(prompt))
        );
    }
    Some(words)
}

#[cfg(test)]
#[path = "figures_tests.rs"]
mod tests;
