//! The question a command asks before installing the Visual C++ runtime:
//! asked in a terminal, answered ahead of time by `--yes`, and with nobody
//! to ask never read, saying what to run instead.

use gglib_core::domain::RuntimeKind;

use super::{Asker, QUESTION, RepairAction, agreed, needs_runtime, repair_action, unasked_line};

/// A yes given ahead of time installs; a terminal is asked; anything else
/// is told.
#[test]
fn a_terminal_is_asked_yes_installs_and_nobody_is_told() {
    assert_eq!(repair_action(true, false), RepairAction::Install);
    assert_eq!(repair_action(true, true), RepairAction::Install);
    assert_eq!(repair_action(false, true), RepairAction::Ask);
    assert_eq!(repair_action(false, false), RepairAction::TellUnasked);
}

/// The question is asked only when there is someone to ask, and a no
/// installs nothing.
#[test]
fn only_a_yes_installs() {
    let never = || -> anyhow::Result<bool> { panic!("nobody is there to be asked") };
    let up = Asker::Up;

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
        unasked_line(Asker::Up),
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
