//! What `config llama` and `config sd` share: the one install, which is also
//! the update, and the offer a command makes before it needs an engine.
//!
//! What to do is `gglib_runtime::engine`'s to decide, for either engine
//! alike: whether there is anything to install, and whether it is a download
//! or a source build. What is each engine's own is how its download and its
//! build are run and drawn, in `llama_install` and `sd`. Nothing here
//! replaces an engine while the daemon serves a model on it.

use anyhow::{Result, bail};
use gglib_app_services::types::ServerInfo;
use gglib_core::domain::RuntimeKind;
use gglib_core::paths::{
    LLAMA_INSTALL_COMMAND, SD_INSTALL_COMMAND, llama_server_path, sd_server_path,
};
use gglib_runtime::engine::{
    InstallPlan, PinStanding, PinState, pin_state, plan_install, standing_line,
};
use gglib_runtime::llama::build_tool_install_lines;

use super::llama_events::{
    built_and_installed, built_in_passing, downloaded_and_installed, downloaded_in_passing,
};
use super::{llama_install, sd};
use crate::bootstrap::CliContext;
use crate::daemon_client;
use crate::utils::input;

/// Whether an install is the command the user ran, which ends by saying
/// what can be done now, or one a command makes on its way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Telling {
    Command,
    InPassing,
}

/// `gglib config <engine> install`, which `update` is too: install the
/// pinned release when what is there is missing or older, and with `force`
/// whatever is there. `build` asks for a source build; `force` also stands
/// for the user's yes to one.
pub(super) async fn install(
    ctx: &CliContext,
    kind: RuntimeKind,
    force: bool,
    build: bool,
) -> Result<()> {
    let state = pin_state(kind)?;
    if !force && !state.standing.wants_install() {
        for line in nothing_to_do_lines(&state, build) {
            println!("{line}");
        }
        return Ok(());
    }
    if let Some(model) = serving_on_the_daemon(ctx, kind).await? {
        bail!(serving_refusal(kind, &model));
    }
    let plan = plan_install(kind, &state, force, build);
    for line in plan_lines(kind, &plan) {
        println!("{line}");
    }
    run(kind, &plan, force, Telling::Command).await
}

/// Make sure `kind` is there before a command needs it: offer the install
/// when it is missing, and the pinned release when what is there is older.
///
/// `yes` is the user's agreement given ahead of time (`gglib up --yes`):
/// nothing is asked, and everything else is still printed. Declining a
/// missing engine is an error, since the command cannot go on without it;
/// declining an update keeps what is there, and so does an update that
/// fails while the engine it would replace is still installed. The update
/// is not offered while the daemon serves a model on the engine, nor when
/// what it serves cannot be asked.
pub(crate) async fn ensure_installed(ctx: &CliContext, kind: RuntimeKind, yes: bool) -> Result<()> {
    let state = pin_state(kind)?;
    match state.standing {
        PinStanding::Missing => {}
        PinStanding::Older => match serving_on_the_daemon(ctx, kind).await {
            Ok(None) => {}
            Ok(Some(model)) => {
                println!("{}", not_while_serving(kind, &state, &model));
                return Ok(());
            }
            Err(_) => return Ok(()),
        },
        PinStanding::AtPin | PinStanding::Newer | PinStanding::Unknown => return Ok(()),
    }

    let plan = plan_install(kind, &state, false, false);
    let (lines, question) = offer(kind, &state, &plan);
    for line in lines {
        println!("{line}");
    }
    if !yes && !input::prompt_confirmation_default_yes(question)? {
        if state.standing == PinStanding::Missing {
            bail!(required_refusal(kind));
        }
        println!("Keeping {} {}.", kind.label(), installed_release(&state));
        return Ok(());
    }
    // The yes was given above, so a source build does not ask again.
    let installed = run(kind, &plan, true, Telling::InPassing).await;
    match installed {
        Err(e) if state.standing == PinStanding::Older && still_runs(kind) => {
            println!(
                "\u{26a0}\u{fe0f}  The update failed: {e:#}\n   Going on with {} {}.",
                kind.label(),
                installed_release(&state)
            );
            Ok(())
        }
        other => other,
    }
}

/// Whether `kind`'s installed server still answers `--version`: after an
/// update that failed, whether what was there before is intact, or was
/// half replaced.
fn still_runs(kind: RuntimeKind) -> bool {
    match kind {
        RuntimeKind::Llama => llama_server_path()
            .is_ok_and(|path| gglib_runtime::llama::validate_llama_binary(&path).is_ok()),
        RuntimeKind::StableDiffusion => {
            gglib_runtime::sd::sd_status().is_ok_and(|status| status.version_line.is_some())
        }
    }
}

/// Carry `plan` out for `kind`. `agreed` is the user's yes to a source
/// build, which then does not ask.
async fn run(kind: RuntimeKind, plan: &InstallPlan, agreed: bool, telling: Telling) -> Result<()> {
    let command = telling == Telling::Command;
    match (kind, plan) {
        (RuntimeKind::Llama, InstallPlan::Download { .. }) => {
            let ending = if command {
                downloaded_and_installed
            } else {
                downloaded_in_passing
            };
            llama_install::install_prebuilt(ending).await
        }
        (RuntimeKind::Llama, InstallPlan::Build { .. }) => {
            let ending = if command {
                built_and_installed
            } else {
                built_in_passing
            };
            llama_install::build_from_source(agreed, ending).await
        }
        (RuntimeKind::StableDiffusion, InstallPlan::Download { .. }) => {
            sd::install_prebuilt().await
        }
        (RuntimeKind::StableDiffusion, InstallPlan::Build { .. }) => {
            sd::build_from_source(agreed).await
        }
    }
}

/// The model the daemon serving this data root runs on `kind`'s server, when
/// that daemon is up. With none, nothing of gglib's runs on it: every model
/// is launched by a daemon.
///
/// That daemon is the one whose token is under this data root, as for
/// `library_changes`: with no token there is no such daemon, and whatever
/// holds the daemon's port is asked nothing.
pub(super) async fn serving_on_the_daemon(
    ctx: &CliContext,
    kind: RuntimeKind,
) -> Result<Option<String>> {
    if daemon_client::auth::daemon_token().is_none() {
        return Ok(None);
    }
    let Ok(daemon) = daemon_client::running(ctx).await else {
        return Ok(None);
    };
    Ok(model_on(kind, &daemon.list_servers().await?))
}

/// The first server in `servers` that `kind`'s server runs, by model name.
fn model_on(kind: RuntimeKind, servers: &[ServerInfo]) -> Option<String> {
    servers
        .iter()
        .find(|server| server.runtime == kind)
        .map(|server| server.model_name.clone())
}

/// Why an install is refused while `model` runs on `kind`'s server.
fn serving_refusal(kind: RuntimeKind, model: &str) -> String {
    let server = kind.server_name();
    format!(
        "{model} is running on {server}. Stop it before installing {}: replacing {server} under a \
         running server leaves it serving from files that are gone.",
        kind.label()
    )
}

/// Why a removal is refused while `model` runs on `kind`'s server: removing
/// the binary and its library under a running server leaves it serving from
/// deleted files.
pub(super) fn removal_refusal(kind: RuntimeKind, model: &str) -> String {
    format!(
        "{model} is running on {}. Stop it before removing {}.",
        kind.server_name(),
        kind.label()
    )
}

/// Why the update a command would offer is not offered while `model` runs
/// on `kind`'s server.
fn not_while_serving(kind: RuntimeKind, state: &PinState, model: &str) -> String {
    format!(
        "{} {} is older than the pinned {}. It is not updated while {model} runs on it.",
        kind.label(),
        installed_release(state),
        pinned(state)
    )
}

/// What a command says when it cannot go on without `kind` and the user
/// declined to install it.
fn required_refusal(kind: RuntimeKind) -> String {
    match kind {
        RuntimeKind::Llama => format!(
            "llama.cpp is required to run this command. Run '{LLAMA_INSTALL_COMMAND}' manually."
        ),
        RuntimeKind::StableDiffusion => format!(
            "stable-diffusion.cpp is required to serve an image model. Run '{SD_INSTALL_COMMAND}' \
             manually."
        ),
    }
}

/// What an install with nothing to do says: where the install stands, and
/// how to install the pin anyway: as a source build when `build` asked for
/// one or a source build is what is there, since `--force` alone takes the
/// download.
fn nothing_to_do_lines(state: &PinState, build: bool) -> Vec<String> {
    let mut lines: Vec<String> = standing_line(state).into_iter().collect();
    let flags = if build || state.built == Some(true) {
        "--build --force"
    } else {
        "--force"
    };
    lines.push(if state.standing == PinStanding::AtPin {
        format!("Use {flags} to reinstall it.")
    } else {
        format!("Use {flags} to install the pinned release.")
    });
    lines
}

/// What an install says before it starts: the download's warning and what
/// it fetches, or why a build was not asked for and is happening anyway.
fn plan_lines(kind: RuntimeKind, plan: &InstallPlan) -> Vec<String> {
    match plan {
        InstallPlan::Download {
            description,
            warning,
        } => {
            let mut lines: Vec<String> = warning
                .iter()
                .map(|warning| format!("\u{26a0}\u{fe0f}  {warning}"))
                .collect();
            lines.push(format!(
                "Downloading the pre-built {} for {description}...",
                kind.label()
            ));
            lines
        }
        InstallPlan::Build {
            no_download: Some(reason),
        } => vec![format!(
            "{reason}; building {} from source instead.",
            kind.server_name()
        )],
        InstallPlan::Build { no_download: None } => Vec::new(),
    }
}

/// What a command says before asking whether to install, and the question.
///
/// The cost of a source build is among the lines and not in the question,
/// because a caller that answers ahead of time never shows the question:
/// the first thing `gglib up --yes` did would otherwise be to start a
/// half-hour compile without a word.
fn offer(kind: RuntimeKind, state: &PinState, plan: &InstallPlan) -> (Vec<String>, &'static str) {
    let product = kind.label();
    let mut lines = vec![String::new()];
    if state.standing == PinStanding::Missing {
        let needs = match kind {
            RuntimeKind::Llama => "chat models need its llama-server",
            RuntimeKind::StableDiffusion => "image models need its sd-server",
        };
        lines.push(format!(
            "\u{26a0}\u{fe0f}  {product} is not installed; {needs}."
        ));
        if let Ok(path) = server_path(kind) {
            lines.push(format!("   Server path: {}", path.display()));
        }
    } else {
        lines.push(format!(
            "\u{2139}\u{fe0f}  {product} {} is installed; gglib now uses {}.",
            installed_release(state),
            pinned(state)
        ));
    }
    lines.push(String::new());

    let build_takes = [
        format!("   This compiles {product} for your hardware and can take 15-30 minutes."),
        "   It happens once; later runs reuse the binaries.".to_owned(),
        String::new(),
    ];
    let question = match plan {
        InstallPlan::Download {
            description,
            warning,
        } => {
            lines.push(format!(
                "A pre-built {product} {} is available for {description}.",
                pinned(state)
            ));
            lines.extend(
                warning
                    .iter()
                    .map(|warning| format!("\u{26a0}\u{fe0f}  {warning}")),
            );
            lines.push(String::new());
            "Download it now?"
        }
        InstallPlan::Build {
            no_download: Some(reason),
        } => {
            lines.push(reason.clone());
            lines.push(String::new());
            lines.push(format!(
                "{product} will be built from source for this machine."
            ));
            lines.extend(build_takes);
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
            "Build it now?"
        }
        InstallPlan::Build { no_download: None } => {
            lines.push(format!(
                "{product} was built from source here, so it is built from source again."
            ));
            lines.extend(build_takes);
            "Build it now?"
        }
    };
    (lines, question)
}

fn server_path(kind: RuntimeKind) -> Result<std::path::PathBuf> {
    Ok(match kind {
        RuntimeKind::Llama => llama_server_path()?,
        RuntimeKind::StableDiffusion => sd_server_path()?,
    })
}

/// The release installed, as status says it.
fn installed_release(state: &PinState) -> &str {
    state.installed_release.as_deref().unwrap_or("unknown")
}

/// The release an install would fetch.
fn pinned(state: &PinState) -> &str {
    state
        .target_release
        .as_deref()
        .unwrap_or("upstream's newest")
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
