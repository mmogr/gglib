//! llama.cpp pre-built binary install — CLI surface adapter.
//!
//! Wraps [`download_prebuilt_binaries`] with the only CLI concern it has:
//! rendering [`LlamaProgressEvent`] as `indicatif` output. The pipeline itself
//! lives in `gglib-runtime::llama` and knows nothing about a terminal.

use std::time::Duration;

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::mpsc;

use gglib_core::download::{format_duration, format_rate};
use gglib_core::paths::llama_server_path;
use gglib_runtime::llama::{InstallPhase, LlamaProgressEvent, download_prebuilt_binaries};

/// Download and install pre-built llama.cpp binaries, rendering progress.
pub(crate) async fn install() -> Result<()> {
    let (tx, rx) = mpsc::channel::<LlamaProgressEvent>(64);
    let install = tokio::spawn(download_prebuilt_binaries(tx));
    consume_install_events_cli(rx).await;
    install.await?
}

/// Consumes [`LlamaProgressEvent`] values from the install pipeline channel and
/// renders them as an `indicatif` spinner or byte progress bar.
///
/// A single `Option<ProgressBar>` tracks the active indicator. Phases are
/// strictly sequential so there is never more than one active bar at a time.
///
/// Speed and time remaining are printed exactly as they arrive. Deriving them
/// here from successive byte counts is what the event type exists to stop.
async fn consume_install_events_cli(mut rx: mpsc::Receiver<LlamaProgressEvent>) {
    let spinner_style = ProgressStyle::default_spinner()
        .template("{spinner:.green} [{elapsed_precise}] {msg}")
        .expect("valid spinner template");

    let bar_style = ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} {msg}",
        )
        .expect("valid bar template")
        .progress_chars("#>-");

    let mut active: Option<ProgressBar> = None;

    while let Some(event) = rx.recv().await {
        match event {
            LlamaProgressEvent::PhaseStarted { phase } => {
                // Clean up any previous indicator before starting a new one.
                if let Some(pb) = active.take() {
                    pb.finish_and_clear();
                }
                let pb = if phase == InstallPhase::Download {
                    // Length is unknown until the first Progress event.
                    let pb = ProgressBar::new(0);
                    pb.set_style(bar_style.clone());
                    pb
                } else {
                    let pb = ProgressBar::new_spinner();
                    pb.set_style(spinner_style.clone());
                    pb.enable_steady_tick(Duration::from_millis(100));
                    pb
                };
                pb.set_message(phase.label());
                active = Some(pb);
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
            LlamaProgressEvent::PhaseCompleted { .. } => {
                if let Some(pb) = active.take() {
                    pb.finish_and_clear();
                }
            }
            LlamaProgressEvent::Completed { version } => {
                if let Some(pb) = active.take() {
                    pb.finish_and_clear();
                }
                println!();
                println!("✓ llama.cpp installed successfully!");
                println!("  Version: {version}");
                if let Ok(server_path) = llama_server_path() {
                    println!("  Server:  {}", server_path.display());
                }
                println!();
                println!("You can now use 'gglib serve', 'gglib proxy', and 'gglib chat'.");
            }
            LlamaProgressEvent::Failed { message } => {
                if let Some(pb) = active.take() {
                    pb.finish_and_clear();
                }
                eprintln!("✗ Install failed: {message}");
            }
        }
    }
}
