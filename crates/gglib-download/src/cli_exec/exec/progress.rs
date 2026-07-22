//! CLI progress rendering for direct (non-queued) downloads.
//!
//! Pure sync, presentation-only module — no knowledge of Python or protocol.
//! Used by the no-callback path (`model upgrade`), where there is no download
//! manager to compute progress for us. The queued path renders through
//! [`crate::cli_emitter::CliDownloadEventEmitter`] instead.
//!
//! Both renderers get their speed and ETA from
//! [`RateEstimator`](gglib_core::download::RateEstimator) and format them with
//! the shared [`format_rate`] / [`format_duration`]. This module owns no rate
//! math of its own — an earlier private exponentially-weighted average here
//! was one of three competing implementations that disagreed with each other.

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use gglib_core::download::{RateEstimator, format_duration, format_rate};
use indicatif::{HumanBytes, ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};

/// Minimum gap between redraws on the non-terminal path.
const PLAIN_MIN_INTERVAL: Duration = Duration::from_millis(250);

// ============================================================================
// CLI Progress Printer
// ============================================================================

/// CLI progress display that automatically selects terminal or plain output.
pub struct CliProgressPrinter {
    inner: ProgressRender,
    /// Shared across both renderers — the rate is a property of the transfer,
    /// not of how it happens to be drawn.
    estimator: RateEstimator,
}

enum ProgressRender {
    Fancy(FancyProgress),
    Plain(PlainProgress),
}

impl CliProgressPrinter {
    /// Create a new progress printer, auto-detecting terminal capability.
    ///
    /// Checks stderr, not stdout: the bar itself draws to stderr (see
    /// [`FancyProgress::new`]), matching the queued-download path's
    /// `CliDownloadEventEmitter`, which uses indicatif's stderr default. A
    /// redirected stdout (`gglib model upgrade ... > file.txt`) should not
    /// silently downgrade the bar when stderr is still an attended terminal.
    #[must_use]
    pub fn new() -> Self {
        let inner = if io::stderr().is_terminal() {
            ProgressRender::Fancy(FancyProgress::new())
        } else {
            ProgressRender::Plain(PlainProgress::new())
        };
        Self {
            inner,
            estimator: RateEstimator::new(Instant::now()),
        }
    }

    /// Update progress display with current download state.
    pub fn update(&mut self, label: Option<&str>, downloaded: u64, total: u64) {
        self.estimator.record(downloaded, total, Instant::now());
        let rate = Rate {
            speed_bps: self.estimator.rate_bps(),
            eta_seconds: self.estimator.eta_seconds(),
        };

        match &mut self.inner {
            ProgressRender::Fancy(inner) => inner.update(label, downloaded, total, &rate),
            ProgressRender::Plain(inner) => inner.update(label, downloaded, total, &rate),
        }
    }

    /// Finish and clear the progress display.
    pub fn finish(&mut self) {
        match &mut self.inner {
            ProgressRender::Fancy(inner) => inner.finish(),
            ProgressRender::Plain(inner) => inner.finish(),
        }
    }
}

impl Default for CliProgressPrinter {
    fn default() -> Self {
        Self::new()
    }
}

/// The estimator's current verdict, ready for display.
struct Rate {
    speed_bps: Option<f64>,
    eta_seconds: Option<f64>,
}

impl Rate {
    /// Render as `118.4 MB/s · ETA 2m 40s`, with placeholders when unknown.
    fn display(&self) -> String {
        format!(
            "{} · ETA {}",
            format_rate(self.speed_bps),
            format_duration(self.eta_seconds)
        )
    }
}

// ============================================================================
// Fancy Terminal Progress (indicatif)
// ============================================================================

struct FancyProgress {
    bar: ProgressBar,
    saw_length: bool,
    last_label: Option<String>,
}

impl FancyProgress {
    fn new() -> Self {
        // Stderr, matching CliDownloadEventEmitter's MultiProgress (indicatif's
        // stderr default) — see the module doc on `CliProgressPrinter::new`.
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr());
        bar.set_style(Self::spinner_style());
        bar.set_message("Preparing fast download".to_string());
        bar.enable_steady_tick(Duration::from_millis(120));
        Self {
            bar,
            saw_length: false,
            last_label: None,
        }
    }

    fn update(&mut self, label: Option<&str>, downloaded: u64, total: u64, rate: &Rate) {
        let label_text = label.filter(|s| !s.is_empty()).unwrap_or("fast download");
        if total == 0 {
            self.bar
                .set_message(format!("{} (preparing...)", Self::format_label(label_text)));
            self.last_label = None;
            self.bar.tick();
            return;
        }

        // The rate changes every tick, so the message is always rewritten.
        self.bar.set_message(format!(
            "{} {}",
            Self::format_label(label_text),
            rate.display()
        ));
        self.last_label = Some(label_text.to_string());

        if self.saw_length {
            if self.bar.length() != Some(total) {
                self.bar.set_length(total);
            }
        } else {
            self.bar.set_style(Self::bar_style());
            self.bar.set_length(total);
            self.saw_length = true;
        }

        self.bar.set_position(downloaded.min(total));
    }

    fn finish(&self) {
        self.bar.finish_and_clear();
    }

    fn spinner_style() -> ProgressStyle {
        ProgressStyle::with_template("⚡ {msg} {spinner}").unwrap()
    }

    /// Note the absence of `{binary_bytes_per_sec}` and `{eta}` — those are
    /// indicatif's own estimates. Rate and ETA come from the shared estimator
    /// and are rendered into `{msg}`; `{human_bytes}` are sizes, which stay
    /// binary.
    fn bar_style() -> ProgressStyle {
        ProgressStyle::with_template(
            "⚡ {msg} {bar:28.cyan/blue} {human_bytes:>9} / {human_total:>9} ({percent:>5.1}%)",
        )
        .unwrap()
        .with_key(
            "human_bytes",
            |state: &ProgressState, w: &mut dyn std::fmt::Write| {
                let _ = write!(w, "{}", HumanBytes(state.pos()));
            },
        )
        .with_key(
            "human_total",
            |state: &ProgressState, w: &mut dyn std::fmt::Write| {
                let value = state
                    .len()
                    .map_or_else(|| "?".to_string(), |len| HumanBytes(len).to_string());
                let _ = write!(w, "{value}");
            },
        )
    }

    fn format_label(raw: &str) -> String {
        const MAX_LABEL: usize = 40;
        let char_count = raw.chars().count();
        if char_count <= MAX_LABEL {
            return raw.to_string();
        }
        // Truncate to MAX_LABEL - 1 chars and add ellipsis
        let mut buf: String = raw.chars().take(MAX_LABEL - 1).collect();
        buf.push('…');
        buf
    }
}

// ============================================================================
// Plain Progress (non-terminal)
// ============================================================================

struct PlainProgress {
    last_emit: Instant,
    last_line_len: usize,
    printed: bool,
}

impl PlainProgress {
    fn new() -> Self {
        Self {
            last_emit: Instant::now(),
            last_line_len: 0,
            printed: false,
        }
    }

    fn update(&mut self, label: Option<&str>, downloaded: u64, total: u64, rate: &Rate) {
        use std::fmt::Write;

        let now = Instant::now();
        if downloaded < total && now.duration_since(self.last_emit) < PLAIN_MIN_INTERVAL {
            return;
        }
        self.last_emit = now;

        let mut line = String::from("⚡ Fast download");
        if let Some(name) = label.filter(|name| !name.is_empty()) {
            let _ = write!(line, " [{name}]");
        }

        if total == 0 {
            let _ = write!(line, ": {} downloaded", HumanBytes(downloaded));
        } else if downloaded == 0 {
            let _ = write!(line, ": Preparing... ({})", HumanBytes(total));
        } else {
            #[allow(clippy::cast_precision_loss)]
            let percent = (downloaded as f64 / total as f64) * 100.0;
            let _ = write!(
                line,
                ": {} / {} ({percent:5.1}%) @ {}",
                HumanBytes(downloaded),
                HumanBytes(total),
                rate.display(),
            );
        }

        let pad = self.last_line_len.saturating_sub(line.len());
        eprint!("\r{line}{:pad$}", "", pad = pad);
        io::stderr().flush().ok();

        self.last_line_len = line.len();
        self.printed = true;
    }

    fn finish(&mut self) {
        if self.printed {
            eprintln!();
            self.printed = false;
            self.last_line_len = 0;
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_label_short() {
        let result = FancyProgress::format_label("model.gguf");
        assert_eq!(result, "model.gguf");
    }

    #[test]
    fn test_format_label_truncates_long() {
        let long_name = "a".repeat(50);
        let result = FancyProgress::format_label(&long_name);
        // Check char count, not byte length (ellipsis is multi-byte)
        assert!(result.chars().count() <= 40);
        assert!(result.ends_with('…'));
    }

    #[test]
    fn rate_display_shows_placeholders_before_warmup() {
        let rate = Rate {
            speed_bps: None,
            eta_seconds: None,
        };
        assert_eq!(rate.display(), "— · ETA —");
    }

    #[test]
    fn rate_display_uses_the_shared_decimal_formatter() {
        let rate = Rate {
            speed_bps: Some(118_400_000.0),
            eta_seconds: Some(160.0),
        };
        assert_eq!(rate.display(), "118.4 MB/s · ETA 2m 40s");
    }

    #[test]
    fn printer_reports_no_rate_from_a_single_sample() {
        // A resumed download's first event carries everything already on disk.
        // Counting that as bytes transferred "just now" is what produced
        // multi-GB/s readings.
        let mut printer = CliProgressPrinter::new();
        printer.update(
            Some("model.gguf"),
            2 * 1024 * 1024 * 1024,
            4 * 1024 * 1024 * 1024,
        );
        assert_eq!(printer.estimator.rate_bps(), None);
    }
}
