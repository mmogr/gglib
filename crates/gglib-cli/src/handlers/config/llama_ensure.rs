//! The install a command offers when it needs llama.cpp and finds none.
//!
//! `gglib serve` and `gglib up` call [`ensure_installed`] before they need
//! `llama-server`. It says what it found, asks, and installs the way `config
//! llama install` would on this machine: the same choice between a download
//! and a build, the same fallback, the same dependency check and the same
//! progress.

use anyhow::Result;
use gglib_core::paths::{is_prebuilt_binary, llama_server_path};
use gglib_runtime::llama::{build_tool_install_lines, check_prebuilt_availability};

use super::llama_events::{built_in_passing, downloaded_in_passing};
use super::llama_install::{build_from_source_impl, install_prebuilt};
use super::llama_method::{AccelerationFlags, InstallMethod, choose_install_method, install_by};
use crate::utils::input;

/// Ensure that llama.cpp binaries are installed, installing them if the
/// user agrees.
///
/// `yes` is the user's agreement given ahead of time (`gglib up --yes`):
/// nothing is asked, and everything else is still printed. Declining is an
/// error, since the command that called cannot go on without the binaries.
pub(crate) async fn ensure_installed(yes: bool) -> Result<()> {
    let server_path = llama_server_path()?;

    if server_path.exists() {
        return Ok(());
    }

    println!();
    println!("⚠️  llama.cpp binaries not found.");
    println!("   Server path: {}", server_path.display());
    println!();

    // No flag is this caller's to pass: the machine decides how to install
    // and which acceleration to build for.
    let detect = AccelerationFlags::default();
    let method = choose_install_method(
        false,
        detect,
        !is_prebuilt_binary(),
        check_prebuilt_availability,
    );

    let (said_first, question) = offer(&method);
    for line in said_first {
        println!("{line}");
    }
    if !yes && !input::prompt_confirmation_default_yes(question)? {
        anyhow::bail!(
            "llama.cpp is required to run this command. Run 'gglib config llama install' manually."
        );
    }
    if matches!(method, InstallMethod::Source { .. }) {
        println!("Building llama.cpp from source (auto-detecting hardware)...");
        println!();
    }

    install_by(
        &method,
        async || install_prebuilt(downloaded_in_passing).await,
        // The yes was given above, so the build does not ask again.
        async || build_from_source_impl(detect, true, built_in_passing).await,
    )
    .await
}

/// What is said before asking, and the question asked.
///
/// The cost of a source build is among the lines and not in the question,
/// because a caller that answers ahead of time never shows the question: the
/// first thing `gglib up --yes` did would otherwise be to start a
/// half-hour compile without a word.
fn offer(method: &InstallMethod) -> (Vec<String>, &'static str) {
    const BUILD_TAKES: [&str; 4] = [
        "",
        "   This compiles llama.cpp for your hardware and typically takes",
        "   15-30 minutes. It happens once; later runs reuse the binaries.",
        "",
    ];

    let mut lines: Vec<String> = Vec::new();
    let question = match method {
        InstallMethod::Prebuilt { description } => {
            lines.push(format!(
                "Pre-built llama.cpp binaries are available for {description}."
            ));
            lines.push(String::new());
            "Would you like to download them now?"
        }
        InstallMethod::Source { no_prebuilt: None } => {
            lines.push(
                "Running from source repository - will build llama.cpp from source.".to_owned(),
            );
            lines.extend(BUILD_TAKES.map(str::to_owned));
            "Would you like to install llama.cpp now?"
        }
        InstallMethod::Source {
            no_prebuilt: Some(reason),
        } => {
            lines.push(reason.clone());
            lines.push(String::new());
            lines
                .push("llama.cpp will be built from source to enable GPU acceleration.".to_owned());
            lines.extend(BUILD_TAKES.map(str::to_owned));
            lines.extend(
                [
                    "Required build tools:",
                    "  • git - for cloning the repository",
                    "  • cmake - for build configuration",
                    "  • g++ or clang++ - for compilation",
                    "",
                ]
                .map(str::to_owned),
            );
            lines.extend(build_tool_install_lines());
            lines.push(String::new());
            "Would you like to build llama.cpp now?"
        }
    };
    (lines, question)
}

#[cfg(test)]
#[path = "llama_ensure_tests.rs"]
mod tests;
