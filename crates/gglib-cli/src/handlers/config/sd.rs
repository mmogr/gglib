//! `gglib config sd`: install, report and remove stable-diffusion.cpp's
//! `sd-server`, the runtime that serves image models — CLI surface adapter.
//!
//! Installing, which is also updating, is `engine`'s, shared with
//! llama.cpp: whether there is anything to install and whether it is a
//! download or a build are decided there for both. What is stable-diffusion.cpp's
//! own is here: running its download and its build, drawn with
//! `llama_events`' renderers under its own name, its status, and its
//! removal, which asks the daemon that serves this data root, when it is up,
//! whether an image model is running, as the web removal does.

use anyhow::{Result, bail};
use tokio::sync::mpsc;

use super::engine;
use super::llama_events::{render_build_events, render_install_events};
use super::llama_install::go_ahead;
use crate::bootstrap::CliContext;
use crate::sd_commands::SdCommand;
use crate::utils::input;
use gglib_core::domain::RuntimeKind;
use gglib_core::paths::{SD_INSTALL_COMMAND, sd_server_path};
use gglib_runtime::llama::{
    Acceleration, BuildEvent, LlamaProgressEvent, UninstallOutcome, build_tool_install_lines,
    build_tools, detect_optimal_acceleration, missing_build_tools,
};
use gglib_runtime::sd::{
    SdStatus, install_sd_prebuilt, run_sd_source_build, sd_files_present, sd_status, uninstall_sd,
};

/// The product, as every line here names it.
pub(super) const PRODUCT: &str = RuntimeKind::StableDiffusion.label();

/// The spinner over the clone of a source build.
const CLONING: Option<&str> = Some("Cloning stable-diffusion.cpp repository...");

const NOW_USABLE: &str = "You can now serve an image model with 'gglib serve <model>'.";

/// Dispatch an `sd` sub-command to its handler.
pub(crate) async fn dispatch(ctx: &CliContext, command: SdCommand) -> Result<()> {
    match command {
        SdCommand::Install { force, build } => {
            engine::install(ctx, RuntimeKind::StableDiffusion, force, build).await
        }
        SdCommand::Status => {
            for line in status_lines(&sd_status()?) {
                println!("{line}");
            }
            Ok(())
        }
        SdCommand::Uninstall { force } => uninstall(ctx, force).await,
    }
}

/// Download and install the pinned pre-built `sd-server`, drawing progress.
pub(super) async fn install_prebuilt() -> Result<()> {
    let (tx, rx) = mpsc::channel::<LlamaProgressEvent>(64);
    let install = tokio::spawn(install_sd_prebuilt(tx));
    render_install_events(rx, PRODUCT, downloaded, &mut std::io::stdout()).await;
    install.await?
}

/// Build `sd-server` from source for what this machine has. `agreed` is the
/// user's yes already given, and nothing is asked.
pub(super) async fn build_from_source(agreed: bool) -> Result<()> {
    let tools = build_tools();
    if !missing_build_tools(&tools).is_empty() {
        for line in missing_tools_lines(&missing_build_tools(&tools)) {
            println!("{line}");
        }
        bail!("Missing required build dependencies");
    }
    let acceleration = detect_optimal_acceleration()?;
    if acceleration == Acceleration::Cpu {
        println!(
            "\u{26a0}\u{fe0f}  No GPU was found, so {PRODUCT} is built for the CPU. Images will take \
             minutes each."
        );
    }
    if !agreed {
        for line in build_lines(acceleration) {
            println!("{line}");
        }
        go_ahead(input::prompt_confirmation_default_yes("Continue?")?)?;
    }

    let (tx, rx) = mpsc::channel::<BuildEvent>(64);
    let build = tokio::spawn(run_sd_source_build(acceleration, tx));
    render_build_events(rx, CLONING, built, &mut std::io::stdout()).await;
    build.await?
}

/// What a source build says it will do before it asks.
fn build_lines(acceleration: Acceleration) -> Vec<String> {
    let name = acceleration.display_name();
    vec![
        "This will:".to_owned(),
        format!("  1. Clone {PRODUCT} and its submodules"),
        format!("  2. Configure with CMake ({name} enabled)"),
        "  3. Compile sd-server (several minutes)".to_owned(),
        String::new(),
    ]
}

/// What is said when the tools a source build needs are missing.
fn missing_tools_lines(missing: &[&str]) -> Vec<String> {
    let mut lines = vec![format!(
        "Building {PRODUCT} needs: {}. Please install:",
        missing.join(", ")
    )];
    lines.push(String::new());
    lines.extend(build_tool_install_lines());
    lines.push(String::new());
    lines.push(format!(
        "After installing, run '{SD_INSTALL_COMMAND}' again."
    ));
    lines
}

/// The ending of a download.
fn downloaded(version: &str) -> Vec<String> {
    let mut lines = vec![
        String::new(),
        format!("\u{2713} {PRODUCT} installed successfully!"),
        format!("  Version: {version}"),
    ];
    if let Ok(server_path) = sd_server_path() {
        lines.push(format!("  Server:  {}", server_path.display()));
    }
    lines.push(String::new());
    lines.push(NOW_USABLE.to_owned());
    lines
}

/// The ending of a source build.
fn built(version: &str, acceleration: &str) -> Vec<String> {
    vec![
        String::new(),
        format!("\u{2713} {PRODUCT} installed successfully!"),
        format!("  Version:       {version}"),
        format!("  Acceleration:  {acceleration}"),
        NOW_USABLE.to_owned(),
    ]
}

/// What `config sd status` prints for `status`.
///
/// Mirrors `config llama status`: installed or not, how (a download or a
/// source build) with its release, platform and date, and then what the
/// binary itself says, with the commit its line names. Not installed says
/// the command that installs it.
fn status_lines(status: &SdStatus) -> Vec<String> {
    if !status.installed {
        return vec![
            "Status: Not installed".to_owned(),
            String::new(),
            format!(
                "Run '{SD_INSTALL_COMMAND}' to install {PRODUCT} ({}).",
                status.pinned_release
            ),
        ];
    }

    let mut lines = vec![
        "Status: Installed".to_owned(),
        format!("Binary: {}", status.binary_path),
    ];
    lines.extend(gglib_runtime::engine::standing_line(&status.pin));
    lines.push(String::new());
    let release = status.release.as_deref().unwrap_or("unknown");
    let platform = status.platform.as_deref().unwrap_or("unknown");
    let when = status.installed_at.as_deref().map(human_time);
    match (status.install_type.as_deref(), &status.record_error) {
        (Some("source"), _) => {
            lines.push("Built from source:".to_owned());
            lines.push(format!("  Release: {release}"));
            lines.push(format!("  Acceleration: {platform}"));
            lines.extend(when.map(|w| format!("  Built: {w}")));
        }
        (Some(_), _) => {
            lines.push("Pre-built download:".to_owned());
            lines.push(format!("  Release: {release}"));
            lines.push(format!("  Platform: {platform}"));
            lines.extend(when.map(|w| format!("  Installed: {w}")));
        }
        (None, Some(e)) => lines.push(format!("Warning: Could not load sd-config.json: {e}")),
        (None, None) => lines.push("Warning: sd-config.json not found".to_owned()),
    }

    lines.push(String::new());
    match &status.version_line {
        Some(line) => {
            lines.push(format!("Binary version: {line}"));
            let commit = status.commit.as_deref().unwrap_or("unidentified");
            lines.push(format!("  Commit: {commit}"));
        }
        None => lines.push("Binary version: sd-server did not answer --version".to_owned()),
    }
    lines
}

/// An RFC 3339 time as `config llama status` prints one; anything else as
/// it stands.
fn human_time(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339).map_or_else(
        |_| rfc3339.to_owned(),
        |time| {
            time.with_timezone(&chrono::Utc)
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string()
        },
    )
}

/// Remove `.sd/`, asking first unless `force`, and print what was removed.
/// Refused while the daemon runs an image model on it, as the web removal
/// is: removing the binary and its library under a running server leaves it
/// serving from deleted files.
pub(super) async fn uninstall(ctx: &CliContext, force: bool) -> Result<()> {
    if !sd_files_present()? {
        println!("{PRODUCT} is not installed.");
        return Ok(());
    }
    if let Some(model) = engine::serving_on_the_daemon(ctx, RuntimeKind::StableDiffusion).await? {
        bail!(engine::removal_refusal(
            RuntimeKind::StableDiffusion,
            &model
        ));
    }
    if !force
        && !input::prompt_confirmation(&format!(
            "This will remove {PRODUCT} and sd-server. Continue?"
        ))?
    {
        println!("Uninstall cancelled.");
        return Ok(());
    }
    for line in uninstall_lines(&uninstall_sd()?) {
        println!("{line}");
    }
    Ok(())
}

/// What an uninstall says it did: each path it removed, then that it is
/// done; or that there was nothing there to remove.
fn uninstall_lines(outcome: &UninstallOutcome) -> Vec<String> {
    if !outcome.was_installed {
        return vec![format!("{PRODUCT} is not installed.")];
    }
    let mut lines: Vec<String> = outcome
        .removed_paths
        .iter()
        .map(|path| format!("\u{2713} Removed {path}"))
        .collect();
    lines.push(format!("{PRODUCT} uninstalled successfully."));
    lines
}

#[cfg(test)]
#[path = "sd_tests.rs"]
mod tests;
