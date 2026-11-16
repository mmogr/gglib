//! The question a command asks before installing the Visual C++ runtime:
//! asked in a terminal, answered ahead of time by `gglib up --yes` and by
//! nothing `gglib config <engine> install` takes, and with nobody to ask
//! never read, saying what to run instead. A command whose install ran ends
//! with the install's words, not the error from before it.

use gglib_core::domain::RuntimeKind;
use gglib_runtime::engine::{RepairReport, RuntimeInstall};

use super::{
    Asker, Offered, QUESTION, RepairAction, agreed, ending, needs_runtime, repair_action,
    unasked_line,
};

/// A yes given ahead of time installs; a terminal is asked; anything else
/// is told.
#[test]
fn a_terminal_is_asked_yes_installs_and_nobody_is_told() {
    assert_eq!(repair_action(true, false), RepairAction::Install);
    assert_eq!(repair_action(true, true), RepairAction::Install);
    assert_eq!(repair_action(false, true), RepairAction::Ask);
    assert_eq!(repair_action(false, false), RepairAction::TellUnasked);
}

/// Only `gglib up --yes`, and the yes of the command an install is made on
/// the way for, answer ahead of time: `gglib config <engine> install` has
/// no yes to give, whatever its flags, so `--force` cannot become one.
#[test]
fn only_a_yes_given_for_it_answers_ahead_of_time() {
    for kind in [RuntimeKind::Llama, RuntimeKind::StableDiffusion] {
        assert!(!Asker::Install(kind).yes());
        assert!(Asker::Passing { kind, yes: true }.yes());
        assert!(!Asker::Passing { kind, yes: false }.yes());
    }
    assert!(Asker::Up { yes: true }.yes());
    assert!(!Asker::Up { yes: false }.yes());
}

/// The question is asked only when there is someone to ask, and a no
/// installs nothing.
#[test]
fn only_a_yes_installs() {
    let never = || -> anyhow::Result<bool> { panic!("nobody is there to be asked") };
    let up = Asker::Up { yes: false };

    assert!(agreed(RepairAction::Install, up, never).unwrap());
    assert!(!agreed(RepairAction::TellUnasked, up, never).unwrap());
    assert!(agreed(RepairAction::Ask, up, || Ok(true)).unwrap());
    assert!(!agreed(RepairAction::Ask, up, || Ok(false)).unwrap());
}

/// What is asked, after the offer has said that Windows will ask too.
#[test]
fn the_question_is_whether_to_install_it() {
    assert_eq!(QUESTION, "Install it?");
}

/// With nobody to ask, each command says what to run, and where Microsoft
/// has it.
#[test]
fn with_nobody_to_ask_each_command_says_what_to_run() {
    assert_eq!(
        unasked_line(Asker::Up { yes: false }),
        "There is no terminal to ask, so nothing was installed. To install it, run 'gglib up \
         --yes', or get it from Microsoft: https://aka.ms/vc14/vc_redist.x64.exe"
    );
    assert_eq!(
        unasked_line(Asker::Install(RuntimeKind::StableDiffusion)),
        "There is no terminal to ask, so nothing was installed. To install it, run 'gglib \
         config sd install' in a terminal, or get it from Microsoft: \
         https://aka.ms/vc14/vc_redist.x64.exe"
    );
}

/// An error that is not an engine's check, or one whose repair is not the
/// runtime, is not offered it.
#[test]
fn other_errors_are_not_offered_the_runtime() {
    assert!(needs_runtime(&anyhow::anyhow!("the download failed")).is_none());
}

fn ran(outcome: RuntimeInstall, works: bool, says: &str) -> Offered {
    Offered::Ran(RepairReport {
        outcome,
        works,
        says: says.to_owned(),
    })
}

/// A command whose install ran ends with the install's words: after 3010
/// with the engine still failing, and after 0 with it still failing, the
/// last line is what the install did, not the error that said to install
/// the runtime. One that made the engine work ends well; one where nothing
/// ran ends with the error it had.
#[test]
fn a_command_ends_with_what_its_install_did() {
    let before = || {
        anyhow::anyhow!(
            "llama-server cannot start: … most likely the Microsoft \
                                     Visual C++ Redistributable (x64), from Microsoft at …"
        )
    };
    let restart = "Installed the Microsoft Visual C++ Redistributable (x64); Windows needs a \
                   restart to finish it. llama-server still cannot start: restart Windows, \
                   then try again.";
    let still = "Installed the Microsoft Visual C++ Redistributable (x64). llama-server still \
                 cannot start, so a file that came with llama.cpp may be missing: reinstall \
                 llama.cpp.";

    for (offered, last) in [
        (ran(RuntimeInstall::RestartNeeded, false, restart), restart),
        (ran(RuntimeInstall::Installed, false, still), still),
    ] {
        let ended = ending(before(), offered).unwrap_err().to_string();
        assert_eq!(ended, last);
        assert!(!ended.contains("from Microsoft at"), "{ended}");
    }
    assert!(ending(before(), ran(RuntimeInstall::Installed, true, "works")).is_ok());
    for offered in [Offered::Nothing, Offered::Declined] {
        let ended = ending(before(), offered).unwrap_err().to_string();
        assert!(ended.contains("from Microsoft at"), "{ended}");
    }
}
