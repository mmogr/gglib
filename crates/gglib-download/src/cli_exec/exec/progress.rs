//! CLI progress rendering for downloads.
//!
//! Pure sync, presentation-only module — no knowledge of Python or protocol.
//! Provides terminal and non-terminal progress display with EWA speed calculation.

use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

use indicatif::{HumanBytes, ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};

// ============================================================================
// Constants
// ============================================================================

const KIB: u64 = 1024;
const MIB: u64 = KIB * 1024;
const GIB: u64 = MIB * 1024;

/// Smoothing factor for exponentially weighted average speed calculation.
const EWA_SMOOTHING: f64 = 0.02;

// ============================================================================
// CLI Progress Printer
// ============================================================================

/// CLI progress display that automatically selects terminal or plain output.
pub struct CliProgressPrinter {
    inner: ProgressRender,
}

enum ProgressRender {
    Fancy(FancyProgress),
    Plain(PlainProgress),
}

impl CliProgressPrinter {
    /// Create a new progress printer, auto-detecting terminal capability.
    pub fn new() -> Self {
        if io::stdout().is_terminal() {
            Self {
                inner: ProgressRender::Fancy(FancyProgress::new()),
            }
        } else {
            Self {
                inner: ProgressRender::Plain(PlainProgress::new()),
            }
        }
    }

    /// Update progress display with current download state.
    pub fn update(&mut self, label: Option<&str>, downloaded: u64, total: u64) {
        match &mut self.inner {
            ProgressRender::Fancy(inner) => inner.update(label, downloaded, total),
            ProgressRender::Plain(inner) => inner.update(label, downloaded, total),
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
        let bar = ProgressBar::with_draw_target(None, ProgressDrawTarget::stdout());
        bar.set_style(Self::spinner_style());
        bar.set_message("Preparing fast download".to_string());
        bar.enable_steady_tick(Duration::from_millis(120));
        Self {
            bar,
            saw_length: false,
            last_label: None,
        }
    }

    fn update(&mut self, label: Option<&str>, downloaded: u64, total: u64) {
        let label_text = label.filter(|s| !s.is_empty()).unwrap_or("fast download");
        if total == 0 {
            self.bar
                .set_message(format!("{} (preparing...)", Self::format_label(label_text)));
            self.last_label = None;
            self.bar.tick();
            return;
        }

        if self.last_label.as_deref() != Some(label_text) {
            self.bar.set_message(Self::format_label(label_text));
            self.last_label = Some(label_text.to_string());
        }

        if !self.saw_length {
            self.bar.set_style(Self::bar_style());
            self.bar.set_length(total);
            self.saw_length = true;
        } else if let Some(current) = self.bar.length() {
            if current != total {
                self.bar.set_length(total);
            }
        } else {
            self.bar.set_length(total);
        }

        self.bar.set_position(downloaded.min(total));
    }

    fn finish(&self) {
        self.bar.finish_and_clear();
    }

    fn spinner_style() -> ProgressStyle {
        ProgressStyle::with_template("⚡ {msg} {spinner}").unwrap()
    }

    fn bar_style() -> ProgressStyle {
        ProgressStyle::with_template(
            "⚡ {msg} {bar:28.cyan/blue} {human_bytes:>9} / {human_total:>9} ({percent:>5.1}%) @ {binary_bytes_per_sec}/s ETA {eta}"
        )
        .unwrap()
        .with_key("human_bytes", |state: &ProgressState, w: &mut dyn std::fmt::Write| {
            let _ = write!(w, "{}", HumanBytes(state.pos()));
        })
        .with_key("human_total", |state: &ProgressState, w: &mut dyn std::fmt::Write| {
            let value = state
                .len()
                .map_or_else(|| "?".to_string(), |len| HumanBytes(len).to_string());
            let _ = write!(w, "{value}");
        })
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
    last_bytes: u64,
    last_line_len: usize,
    printed: bool,
    /// Exponentially weighted average speed in bytes/sec
    ewa_speed: f64,
}

impl PlainProgress {
    fn new() -> Self {
        Self {
            last_emit: Instant::now(),
            last_bytes: 0,
            last_line_len: 0,
            printed: false,
            ewa_speed: 0.0,
        }
    }

    fn update(&mut self, label: Option<&str>, downloaded: u64, total: u64) {
        const MIN_INTERVAL: Duration = Duration::from_millis(250);
        let now = Instant::now();
        let elapsed_since_last = now.duration_since(self.last_emit);

        if downloaded < total && elapsed_since_last < MIN_INTERVAL {
            return;
        }

        // Calculate instantaneous speed for this interval
        let elapsed_secs = elapsed_since_last.as_secs_f64();
        let bytes_delta = downloaded.saturating_sub(self.last_bytes);
        #[allow(clippy::cast_precision_loss)]
        let instant_speed = if elapsed_secs > 0.0 {
            bytes_delta as f64 / elapsed_secs
        } else {
            0.0
        };

        // Update EWA speed
        if self.printed {
            self.ewa_speed =
                EWA_SMOOTHING.mul_add(instant_speed, (1.0 - EWA_SMOOTHING) * self.ewa_speed);
        } else {
            self.ewa_speed = instant_speed;
        }

        self.last_emit = now;
        self.last_bytes = downloaded;

        let speed_mib = self.ewa_speed / (1024.0 * 1024.0);

        let (down_div, down_unit) = pick_display_unit(downloaded);
        let (total_div, total_unit) = pick_display_unit(total);

        #[allow(clippy::cast_precision_loss)]
        let downloaded_val = downloaded as f64 / down_div;
        #[allow(clippy::cast_precision_loss)]
        let total_val = total as f64 / total_div;

        let downloaded_str = format_scaled(downloaded_val, down_unit, downloaded);
        let total_str = format_scaled(total_val, total_unit, total);

        #[allow(clippy::cast_precision_loss)]
        let percent = if total > 0 {
            (downloaded as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let mut line = String::from("⚡ Fast download");
        if let Some(name) = label.filter(|name| !name.is_empty()) {
            use std::fmt::Write;
            let _ = write!(line, " [{name}]");
        }
        if total > 0 {
            use std::fmt::Write;
            if downloaded == 0 {
                let _ = write!(line, ": Preparing... ({total_str} {total_unit})");
            } else {
                let _ = write!(
                    line,
                    ": {downloaded_str} {down_unit} / {total_str} {total_unit} ({percent:5.1}%) @ {speed_mib:5.1} MiB/s"
                );
            }
        } else {
            use std::fmt::Write;
            let _ = write!(line, ": {downloaded_str} {down_unit} downloaded");
        }

        let pad = self.last_line_len.saturating_sub(line.len());
        print!("\r{line}");
        if pad > 0 {
            for _ in 0..pad {
                print!(" ");
            }
        }
        io::stdout().flush().ok();

        self.last_line_len = line.len();
        self.printed = true;
    }

    fn finish(&mut self) {
        if self.printed {
            println!();
            self.printed = false;
            self.last_line_len = 0;
        }
    }
}

// ============================================================================
// Display Unit Helpers
// ============================================================================

/// Select the appropriate display unit based on the reference value.
#[allow(clippy::cast_precision_loss)]
const fn pick_display_unit(reference: u64) -> (f64, &'static str) {
    if reference >= GIB {
        (GIB as f64, "GiB")
    } else if reference >= MIB {
        (MIB as f64, "MiB")
    } else if reference >= KIB {
        (KIB as f64, "KiB")
    } else {
        (1.0, "B")
    }
}

/// Format a scaled value with appropriate precision.
fn format_scaled(value: f64, unit: &str, raw: u64) -> String {
    if unit == "B" {
        return raw.to_string();
    }

    if value >= 100.0 {
        format!("{value:6.1}")
    } else if value >= 10.0 {
        format!("{value:5.2}")
    } else if value >= 1.0 {
        format!("{value:4.2}")
    } else {
        format!("{value:.3}")
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_display_unit_bytes() {
        let (divisor, unit) = pick_display_unit(500);
        assert_eq!(unit, "B");
        assert!((divisor - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pick_display_unit_kib() {
        let (divisor, unit) = pick_display_unit(2 * KIB);
        assert_eq!(unit, "KiB");
        #[allow(clippy::cast_precision_loss)]
        let expected = KIB as f64;
        assert!((divisor - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pick_display_unit_mib() {
        let (divisor, unit) = pick_display_unit(50 * MIB);
        assert_eq!(unit, "MiB");
        #[allow(clippy::cast_precision_loss)]
        let expected = MIB as f64;
        assert!((divisor - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn test_pick_display_unit_gib() {
        let (divisor, unit) = pick_display_unit(2 * GIB);
        assert_eq!(unit, "GiB");
        #[allow(clippy::cast_precision_loss)]
        let expected = GIB as f64;
        assert!((divisor - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn test_format_scaled_bytes() {
        assert_eq!(format_scaled(123.0, "B", 123), "123");
    }

    #[test]
    fn test_format_scaled_large() {
        let result = format_scaled(150.5, "MiB", 150 * MIB);
        assert!(result.contains("150"));
    }

    #[test]
    fn test_format_scaled_small() {
        let result = format_scaled(0.5, "MiB", MIB / 2);
        assert!(result.contains("0.5"));
    }

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
}
