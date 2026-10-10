//! `gglib image`: draw through the daemon, show the render on one live line,
//! save each image, and print where it went.
//!
//! The daemon owns the image model and its job (`daemon_client::images`), so
//! this command starts the daemon when none runs and never drives
//! `sd-server` itself. The line reads queued (with the place in line),
//! loading, sampling step of total (and image of images for more than one),
//! decoding. Where stderr is not a terminal there is no line to redraw, so
//! each of those is printed there once, as a plain line, when it changes
//! ([`PlainLines`]). `sd-server` cannot interrupt a generating job, so
//! Ctrl-C says that the daemon finishes the render and discards it, and
//! exits 130.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context as _, Result, bail};
use gglib_core::ports::{ImageError, ImageGenerationPort, ImageProgress, ImageRequest, ImageStage};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::sync::mpsc;

use crate::bootstrap::CliContext;
use crate::daemon_client::ensure_daemon;
use crate::daemon_client::images::DaemonImageGenerator;
use crate::image_commands::ImageCommandArgs;

/// What Ctrl-C prints before the command exits 130.
pub(crate) const INTERRUPTED: &str =
    "the render cannot be interrupted; the daemon finishes it and discards it";

/// Run `gglib image`.
pub(crate) async fn execute(ctx: &CliContext, args: ImageCommandArgs) -> Result<()> {
    let paths = output_paths(args.output.as_deref(), usize::from(args.n), unix_now());
    if let Some(taken) = paths.iter().find(|p| p.exists()) {
        bail!(
            "{} already exists; name another file with -o",
            taken.display()
        );
    }
    let generator = DaemonImageGenerator::new(ensure_daemon(ctx).await?);
    let request = ImageRequest {
        model: args.model,
        prompt: args.prompt,
        size: args.size,
        n: args.n,
        seed: args.seed,
    };

    let bar = ctx.console.add_bar(ProgressBar::new_spinner());
    bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .expect("valid spinner template"),
    );
    bar.enable_steady_tick(Duration::from_millis(120));
    bar.set_message("starting");

    // The spinner draws only on a terminal; anywhere else each stage and
    // step is said once, on a line of its own.
    let live = ctx.console.draws_bars();
    let mut plain = PlainLines::default();

    let (reports, mut rx) = mpsc::channel(64);
    let render = generator.generate(request, reports);
    tokio::pin!(render);
    let result = loop {
        tokio::select! {
            Some(report) = rx.recv() => {
                if let Some(text) = say(live, &bar, &mut plain, line(&report, args.n)) {
                    eprintln!("{text}");
                }
            }
            result = &mut render => break result,
            _ = tokio::signal::ctrl_c() => {
                ctx.console.remove_bar(&bar);
                eprintln!("\n  {INTERRUPTED}");
                std::process::exit(130);
            }
        }
    };
    ctx.console.remove_bar(&bar);
    let batch = result.map_err(|e| anyhow::anyhow!(failure(&e)))?;
    for (image, path) in batch.images.iter().zip(&paths) {
        save(path, &image.bytes)?;
        println!(
            "[image {}x{}] {}",
            image.width,
            image.height,
            path.display()
        );
    }
    Ok(())
}

/// The live line for one report of a render of `images` images.
pub(crate) fn line(report: &ImageProgress, images: u8) -> String {
    match &report.stage {
        ImageStage::Queued {
            position,
            behind: Some(what),
        } => format!("queued behind {what}, place {position}"),
        ImageStage::Queued { position, .. } => format!("queued, place {position}"),
        ImageStage::Loading => "loading the model…".to_owned(),
        ImageStage::Sampling { pass, step, total } if images > 1 => {
            format!("sampling {step}/{total} (image {pass} of {images})")
        }
        ImageStage::Sampling { step, total, .. } => format!("sampling {step}/{total}"),
        ImageStage::Decoding => "decoding".to_owned(),
        ImageStage::Finishing => "finishing".to_owned(),
    }
}

/// Say `text`, a render's line: on `bar` where the console draws bars
/// (`live`), and anywhere else as the plain line to print, when it differs
/// from the one before.
pub(crate) fn say(
    live: bool,
    bar: &ProgressBar,
    plain: &mut PlainLines,
    text: String,
) -> Option<String> {
    if live {
        bar.set_message(text);
        None
    } else {
        plain.changed(text)
    }
}

/// What a render says where no line can be redrawn: each line once, when
/// it differs from the one before, so a stage is one line and each sampling
/// step another.
#[derive(Debug, Default)]
pub(crate) struct PlainLines {
    last: Option<String>,
}

impl PlainLines {
    /// `text` when it is not the line last printed.
    pub(crate) fn changed(&mut self, text: String) -> Option<String> {
        if self.last.as_deref() == Some(text.as_str()) {
            return None;
        }
        self.last = Some(text.clone());
        Some(text)
    }
}

/// Where each of `count` images is saved: `output`, or `gglib-<unix>.png`
/// here; with more than one, each name gains `-1`, `-2`, … before its
/// extension.
pub(crate) fn output_paths(output: Option<&Path>, count: usize, unix: u64) -> Vec<PathBuf> {
    let base = output.map_or_else(
        || PathBuf::from(format!("gglib-{unix}.png")),
        Path::to_path_buf,
    );
    if count <= 1 {
        return vec![base];
    }
    let stem = base
        .file_stem()
        .map_or_else(|| "gglib".to_owned(), |s| s.to_string_lossy().into_owned());
    let extension = base
        .extension()
        .map_or_else(|| "png".to_owned(), |e| e.to_string_lossy().into_owned());
    (1..=count)
        .map(|i| base.with_file_name(format!("{stem}-{i}.{extension}")))
        .collect()
}

/// The sentence a failed render ends the command with: the daemon's words,
/// which say what to do next, and for a daemon that could not be reached,
/// that.
pub(crate) fn failure(error: &ImageError) -> String {
    match error {
        ImageError::Refused {
            code: Some(code), ..
        } if code == "image_runtime_not_installed" || code == "drawing_unavailable" => {
            format!("cannot draw: {error}")
        }
        ImageError::Refused { status: 0, .. } => error.to_string(),
        _ => format!("the render failed: {error}"),
    }
}

fn save(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write as _;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("could not create {}", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("could not write {}", path.display()))
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
#[path = "image_tests.rs"]
mod tests;
