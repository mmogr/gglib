//! The question a command asks when an engine it needs does not work for
//! want of Microsoft's Visual C++ runtime, which gglib can install: the
//! face of `gglib_runtime::engine::install_visual_cpp_runtime` in a
//! terminal, for `gglib config llama|sd install` and `gglib up`.
//!
//! Which engines need it, and how it is installed and checked, is the
//! runtime's; this decides only whether to ask, and says what happened.
//! Only a terminal is asked: a stdin that is not one may be a pipe that
//! never ends, and reading it would wait forever. With nobody to ask and no
//! yes given ahead of time, nothing is installed and the line says what to
//! run. A yes ahead of time comes only from a command that has one for
//! this: `gglib up --yes`. `gglib config <engine> install` has none, and
//! `--force` is not one; [`Asker::Install`] cannot carry one.
//!
//! When an install ran, the command ends with what it did, never with the
//! error from before it, whose words say to install the runtime.

use std::io::IsTerminal as _;

use anyhow::{Result, anyhow};
use gglib_core::domain::RuntimeKind;
use gglib_core::paths::{LLAMA_INSTALL_COMMAND, SD_INSTALL_COMMAND};
use gglib_core::utils::process::bounded::VISUAL_CPP_RUNTIME_X64_URL;
use gglib_runtime::engine::{Fix, NotWorking, RepairReport, install_visual_cpp_runtime};

use crate::utils::input;

/// The question, after the offer has said what it is and that Windows will
/// ask for permission.
pub(super) const QUESTION: &str = "Install it?";

/// What a command does about a runtime an engine needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RepairAction {
    /// The yes was given ahead of time: install without asking.
    Install,
    /// Ask on the terminal.
    Ask,
    /// Nobody to ask: install nothing, and say what to run.
    TellUnasked,
}

/// The rule: a yes given ahead of time installs, a terminal is asked, and
/// anything else is told.
pub(super) const fn repair_action(yes: bool, stdin_is_terminal: bool) -> RepairAction {
    if yes {
        RepairAction::Install
    } else if stdin_is_terminal {
        RepairAction::Ask
    } else {
        RepairAction::TellUnasked
    }
}

/// Which command asks, with the yes it was given ahead of time, if it has
/// one to give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asker {
    /// `gglib up`; `yes` is its `--yes`.
    Up { yes: bool },
    /// `gglib config <engine> install`, which asks in a terminal and has no
    /// yes ahead of time.
    Install(RuntimeKind),
    /// An install another command makes on its way, with that command's
    /// yes ahead of time.
    Passing { kind: RuntimeKind, yes: bool },
}

impl Asker {
    /// The yes given ahead of time.
    pub(super) const fn yes(self) -> bool {
        match self {
            Self::Up { yes } | Self::Passing { yes, .. } => yes,
            Self::Install(_) => false,
        }
    }
}

/// What a command says when it has nobody to ask.
pub(super) fn unasked_line(asker: Asker) -> String {
    let run = match asker {
        Asker::Up { .. } => "'gglib up --yes'".to_owned(),
        Asker::Install(kind) | Asker::Passing { kind, .. } => format!(
            "'{}' in a terminal",
            match kind {
                RuntimeKind::Llama => LLAMA_INSTALL_COMMAND,
                RuntimeKind::StableDiffusion => SD_INSTALL_COMMAND,
            }
        ),
    };
    format!(
        "There is no terminal to ask, so nothing was installed. To install it, run {run}, or \
         get it from Microsoft: {VISUAL_CPP_RUNTIME_X64_URL}"
    )
}

/// The engine's check that `error` carries, with its fix, when the fix is
/// the runtime's: its install, or Windows' repair of a damaged one.
pub(crate) fn needs_runtime(error: &anyhow::Error) -> Option<(&NotWorking, Fix)> {
    let not_working = error
        .chain()
        .find_map(|cause| cause.downcast_ref::<NotWorking>())?;
    match not_working.fix()? {
        fix @ (Fix::InstallVisualCppRuntime | Fix::RepairVisualCppRuntime) => {
            Some((not_working, fix))
        }
        Fix::Reinstall => None,
    }
}

/// Whether to install, given `action`; `ask` puts the question, and is
/// called only for [`RepairAction::Ask`].
pub(super) fn agreed(
    action: RepairAction,
    asker: Asker,
    ask: impl FnOnce() -> Result<bool>,
) -> Result<bool> {
    match action {
        RepairAction::Install => Ok(true),
        RepairAction::Ask => {
            let yes = ask()?;
            if !yes {
                println!("Nothing was installed.");
            }
            Ok(yes)
        }
        RepairAction::TellUnasked => {
            println!("{}", unasked_line(asker));
            Ok(false)
        }
    }
}

/// What came of an offer of the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Offered {
    /// The error was not the runtime's: nothing was offered.
    Nothing,
    /// The runtime is damaged, which only Windows repairs: nothing was
    /// offered, and this says how Windows repairs it.
    Damaged(String),
    /// Offered, and not agreed to: nothing was installed.
    Declined,
    /// The installer was run, with this report.
    Ran(RepairReport),
}

/// When `error`, from `kind`'s check or install, is the runtime's, say so:
/// offer its install and make it on a yes, or (as the command's ending)
/// say how Windows repairs a damaged one. A report that `kind` works is
/// printed; one that it does not is the command's ending ([`ending`]).
///
/// # Errors
///
/// The answer could not be read.
pub(crate) async fn offer_runtime(
    kind: RuntimeKind,
    error: &anyhow::Error,
    asker: Asker,
) -> Result<Offered> {
    let Some((not_working, fix)) = needs_runtime(error) else {
        return Ok(Offered::Nothing);
    };
    println!();
    println!("{not_working}");
    println!();
    if fix != Fix::InstallVisualCppRuntime {
        return Ok(Offered::Damaged(fix.offer(kind)));
    }
    println!("{}", fix.offer(kind));
    let action = repair_action(asker.yes(), std::io::stdin().is_terminal());
    if !agreed(action, asker, || {
        input::prompt_confirmation_default_yes(QUESTION)
    })? {
        return Ok(Offered::Declined);
    }
    println!("Downloading it from Microsoft...");
    if let Some(waiting) = fix.waiting() {
        println!("{waiting}");
    }
    let report = install_visual_cpp_runtime(kind).await;
    if report.works {
        println!("{}", report.says);
    }
    Ok(Offered::Ran(report))
}

/// How a command that met `error` ends after `offered`: well when an
/// install made the engine work; with the install's own words when one ran
/// and it still does not; with how Windows repairs a damaged runtime; with
/// `error` when nothing ran.
pub(crate) fn ending(error: anyhow::Error, offered: Offered) -> Result<()> {
    match offered {
        Offered::Ran(report) if report.works => Ok(()),
        Offered::Ran(report) => Err(anyhow!(report.says)),
        Offered::Damaged(how) => Err(anyhow!(how)),
        Offered::Nothing | Offered::Declined => Err(error),
    }
}

/// `installed`, an install of `kind` that may have failed its check for
/// want of the runtime: offer the runtime then, and when it makes `kind`
/// work, the install worked.
///
/// # Errors
///
/// The install failed for another reason, or `kind` still does not work.
pub(crate) async fn after_install(
    kind: RuntimeKind,
    installed: Result<()>,
    asker: Asker,
) -> Result<()> {
    match installed {
        Err(e) if needs_runtime(&e).is_some() => {
            let offered = offer_runtime(kind, &e, asker).await?;
            ending(e, offered)
        }
        other => other,
    }
}

#[cfg(test)]
#[path = "repair_tests.rs"]
mod tests;
