//! The question a command asks before installing the Visual C++ runtime:
//! the shapes the tests are written against, with nothing asked yet.

use gglib_core::domain::RuntimeKind;
use gglib_runtime::engine::NotWorking;

/// The question.
pub(super) const QUESTION: &str = "";

/// What a command does about a runtime an engine needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RepairAction {
    Install,
    Ask,
    TellUnasked,
}

/// The rule.
pub(super) const fn repair_action(_yes: bool, _stdin_is_terminal: bool) -> RepairAction {
    RepairAction::Ask
}

/// Which command asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Asker {
    Up,
    Install(RuntimeKind),
}

/// What a command says when it has nobody to ask.
pub(super) fn unasked_line(_asker: Asker) -> String {
    String::new()
}

/// The engine's check that `error` carries, when what it needs is the
/// runtime.
pub(crate) const fn needs_runtime(_error: &anyhow::Error) -> Option<&NotWorking> {
    None
}

/// Whether to install, given `action`.
pub(super) fn agreed(
    _action: RepairAction,
    _asker: Asker,
    ask: impl FnOnce() -> anyhow::Result<bool>,
) -> anyhow::Result<bool> {
    ask()
}

#[cfg(test)]
#[path = "repair_tests.rs"]
mod tests;
