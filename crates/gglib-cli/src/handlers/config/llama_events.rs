//! What a llama.cpp install looks like on a terminal.
//!
//! One renderer for a source build's [`BuildEvent`]s and one for a download's
//! [`LlamaProgressEvent`]s. Every command that builds or downloads llama.cpp
//! hands its channel to one of them: `config llama install`, `rebuild` and
//! `update`, and the install a first `gglib serve` or `gglib up` offers; so
//! does `config sd install`, whose stable-diffusion.cpp reports the same
//! events, with its product's name on the download's bar. A
//! command differs from the next in what it says once the work is done, which
//! is the ending it passes, and in what is drawn while git fetches the
//! checkout: a clone and a pull are one phase, and only the command knows
//! which of them it started.
//!
//! A phase that is drawn is a spinner or a bar, which `indicatif` draws on
//! stderr and takes away again. What is left behind is written to `out`.

use std::borrow::Cow;
use std::io::Write;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::mpsc;

use gglib_core::download::{format_duration, format_rate};
use gglib_core::paths::llama_server_path;
use gglib_runtime::llama::{BuildEvent, BuildPhase, InstallPhase, LlamaProgressEvent};

/// What a command prints when its build ends well, given the version built
/// and the acceleration it was built for.
pub(super) type BuildEnding = fn(version: &str, acceleration: &str) -> Vec<String>;

/// What a command prints when its download ends well, given the release
/// that was installed.
pub(super) type DownloadEnding = fn(version: &str) -> Vec<String>;

/// The spinner over a clone. A clone's own output is piped into the build's
/// log lines, so nothing else on the terminal shows that it is running.
pub(super) const CLONING: Option<&str> = Some("Cloning llama.cpp repository...");

/// No spinner over a pull. git has the terminal for a pull and reports its
/// progress there, where a spinner would be drawn across it.
pub(super) const GIT_REPORTS_THE_PULL: Option<&str> = None;

const NOW_USABLE: &str = "You can now use 'gglib serve', 'gglib proxy', and 'gglib chat'.";

/// The ending of `config llama install` and `rebuild`, built from source.
pub(super) fn built_and_installed(version: &str, acceleration: &str) -> Vec<String> {
    vec![
        String::new(),
        "✓ llama.cpp installed successfully!".to_owned(),
        format!("  Version:       {version}"),
        format!("  Acceleration:  {acceleration}"),
        NOW_USABLE.to_owned(),
    ]
}

/// The ending of `config llama update`.
pub(super) fn updated(version: &str, acceleration: &str) -> Vec<String> {
    vec![
        String::new(),
        "✓ llama.cpp updated successfully!".to_owned(),
        format!("  New version: {version}"),
        format!("  Acceleration: {acceleration}"),
    ]
}

/// The ending of a build another command ran on its way to its own work.
pub(super) fn built_in_passing(version: &str, _acceleration: &str) -> Vec<String> {
    vec![format!("✓ Build complete ({version})")]
}

/// The ending of `config llama install`, downloaded.
pub(super) fn downloaded_and_installed(version: &str) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        "✓ llama.cpp installed successfully!".to_owned(),
        format!("  Version: {version}"),
    ];
    if let Ok(server_path) = llama_server_path() {
        lines.push(format!("  Server:  {}", server_path.display()));
    }
    lines.push(String::new());
    lines.push(NOW_USABLE.to_owned());
    lines
}

/// The ending of a download another command ran on its way to its own work.
pub(super) fn downloaded_in_passing(version: &str) -> Vec<String> {
    vec![format!("✓ llama.cpp installed ({version})")]
}

fn spinner_style() -> ProgressStyle {
    ProgressStyle::default_spinner()
        .template("{spinner:.green} [{elapsed_precise}] {msg}")
        .expect("valid spinner template")
}

/// A bar that counts `counted`, an `indicatif` template key pair such as
/// `{pos}/{len} ({percent}%)`.
fn bar_style(counted: &str) -> ProgressStyle {
    ProgressStyle::default_bar()
        .template(&format!(
            "{{spinner:.green}} [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {counted} {{msg}}"
        ))
        .expect("valid bar template")
        .progress_chars("#>-")
}

fn spinner(message: impl Into<Cow<'static, str>>) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(spinner_style());
    pb.set_message(message);
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// A bar whose length is not known until the first progress event.
fn bar(counted: &str, message: impl Into<Cow<'static, str>>) -> ProgressBar {
    let pb = ProgressBar::new(0);
    pb.set_style(bar_style(counted));
    pb.set_message(message);
    pb
}

/// The indicator a phase of a build is drawn with, if it has one.
///
/// `fetching` is the spinner over the phase in which git fetches the
/// checkout, which is the command's to word or to leave out.
fn build_indicator(phase: BuildPhase, fetching: Option<&'static str>) -> Option<ProgressBar> {
    Some(match phase {
        BuildPhase::Compile => bar("{pos}/{len} ({percent}%)", "Compiling..."),
        BuildPhase::CloneOrUpdateRepo => spinner(fetching?),
        BuildPhase::Configure => spinner("Configuring with CMake..."),
        BuildPhase::InstallBinaries => spinner("Installing binaries..."),
    })
}

/// Take the active indicator off the screen, if there is one.
fn clear(active: &mut Option<ProgressBar>) {
    if let Some(pb) = active.take() {
        pb.finish_and_clear();
    }
}

fn write_lines(out: &mut impl Write, lines: &[String]) {
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
}

/// Render a source build's events until its channel closes.
///
/// A single `Option<ProgressBar>` tracks the active indicator. Phases are
/// strictly sequential so there is never more than one active bar at a time.
/// A log line that arrives while an indicator is drawn is printed above it,
/// and any other goes to `out`. `fetching` is [`CLONING`] or
/// [`GIT_REPORTS_THE_PULL`].
pub(super) async fn render_build_events(
    mut rx: mpsc::Receiver<BuildEvent>,
    fetching: Option<&'static str>,
    ending: BuildEnding,
    out: &mut impl Write,
) {
    let mut active: Option<ProgressBar> = None;

    while let Some(event) = rx.recv().await {
        match event {
            BuildEvent::PhaseStarted { phase } => {
                clear(&mut active);
                active = build_indicator(phase, fetching);
            }
            BuildEvent::PhaseCompleted { .. } => clear(&mut active),
            BuildEvent::Progress { current, total } => {
                if let Some(pb) = &active {
                    pb.set_length(total);
                    pb.set_position(current);
                }
            }
            BuildEvent::Log { message } => {
                if let Some(pb) = &active {
                    pb.println(&message);
                } else {
                    let _ = writeln!(out, "{message}");
                }
            }
            BuildEvent::Completed {
                version,
                acceleration,
            } => {
                clear(&mut active);
                write_lines(out, &ending(&version, &acceleration));
            }
            BuildEvent::Failed { message } => {
                clear(&mut active);
                eprintln!("✗ Build failed: {message}");
            }
        }
    }
}

/// The indicator a phase of `product`'s download is drawn with: a byte bar
/// while the archive downloads and a spinner for every other phase, each
/// labelled as [`InstallPhase::label_for`] words it for `product`.
fn install_indicator(phase: InstallPhase, product: &str) -> ProgressBar {
    let label = phase.label_for(product);
    if phase == InstallPhase::Download {
        bar("{bytes}/{total_bytes}", label)
    } else {
        spinner(label)
    }
}

/// Render a download of `product` (`RuntimeKind::label`) until its channel
/// closes.
///
/// One indicator at a time, as for a build ([`install_indicator`]).
///
/// Speed and time remaining are printed exactly as they arrive. Deriving them
/// here from successive byte counts is what the event type exists to stop.
pub(super) async fn render_install_events(
    mut rx: mpsc::Receiver<LlamaProgressEvent>,
    product: &str,
    ending: DownloadEnding,
    out: &mut impl Write,
) {
    let mut active: Option<ProgressBar> = None;

    while let Some(event) = rx.recv().await {
        match event {
            LlamaProgressEvent::PhaseStarted { phase } => {
                clear(&mut active);
                active = Some(install_indicator(phase, product));
            }
            LlamaProgressEvent::Progress {
                downloaded,
                total,
                rate_bps,
                eta_seconds,
            } => {
                if let Some(pb) = &active {
                    pb.set_length(total);
                    pb.set_position(downloaded);
                    pb.set_message(format!(
                        "{} ({} remaining)",
                        format_rate(rate_bps),
                        format_duration(eta_seconds)
                    ));
                }
            }
            LlamaProgressEvent::PhaseCompleted { .. } => clear(&mut active),
            LlamaProgressEvent::Completed { version } => {
                clear(&mut active);
                write_lines(out, &ending(&version));
            }
            LlamaProgressEvent::Failed { message } => {
                clear(&mut active);
                eprintln!("✗ Install failed: {message}");
            }
        }
    }
}

#[cfg(test)]
#[path = "llama_events_tests.rs"]
mod tests;
