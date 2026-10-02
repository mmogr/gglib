//! Which machine a model is on, what may be done to a model there, and the
//! name a machine is shown by.
//!
//! A model is named by its machine and its catalogue id ([`ModelRef`]). The
//! machine is carried as structure — the `--remote` flag, the
//! `/api/remote/...` prefix, or a [`Machine`] in JSON — and never as text a
//! parser would have to split, so no proxy ever reads a machine out of a
//! model name and a request cannot be passed on a second hop.
//!
//! What a model on the paired machine allows is ADR 0013's line, **use the
//! machine, don't change it**, written once as [`ModelAction::on_paired`].
//!
//! A machine's host name reaches the other side of a pairing as text it wrote
//! about itself, so it is untrusted. [`machine_name`] keeps only a plain host
//! label: anything that is not one is dropped rather than repaired.

use serde::{Deserialize, Serialize};

/// Which machine a model is on.
///
/// There is one pairing (ADR 0013), so a machine is this one or the one it
/// is paired with. Serialised as `{"kind":"local"}` or
/// `{"kind":"paired","fingerprint":"…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Machine {
    /// This machine.
    Local,
    /// The machine this one is paired with.
    Paired {
        /// The ticket fingerprint of that machine. It is the identity: it is
        /// compared, and never shown as the machine's name.
        fingerprint: String,
    },
}

impl Machine {
    /// Whether `action` may be done to a model on this machine: anything on
    /// this one, and on the paired one what [`ModelAction::on_paired`] allows.
    #[must_use]
    pub const fn allows(&self, action: ModelAction) -> bool {
        matches!(self, Self::Local) || action.on_paired()
    }

    /// Every action [`allows`](Self::allows) answers yes to, in
    /// [`ModelAction::ALL`]'s order: the table's row for this machine, as
    /// data a surface can render from.
    #[must_use]
    pub fn actions(&self) -> Vec<ModelAction> {
        ModelAction::ALL
            .into_iter()
            .filter(|action| self.allows(*action))
            .collect()
    }
}

/// What can be done to a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub enum ModelAction {
    /// See it in a list of the machine's models.
    List,
    /// Read everything stored about it.
    Detail,
    /// Send it a turn.
    Chat,
    /// Have it resident now, so the first turn does not wait.
    Load,
    /// Change anything in the machine's library: add, remove, edit, tag,
    /// verify, repair, upgrade, benchmark, set as the default.
    Manage,
}

impl ModelAction {
    /// Every action, in the order a surface lists them.
    pub const ALL: [Self; 5] = [
        Self::List,
        Self::Detail,
        Self::Chat,
        Self::Load,
        Self::Manage,
    ];

    /// Whether this may be done to a model on the paired machine.
    ///
    /// ADR 0013's line: use the machine, don't change it. Reading a model,
    /// sending it a turn and having it resident use the machine; anything
    /// that changes its library does not. Matched without a wildcard, so a
    /// new action does not compile until it has said which side it is on.
    #[must_use]
    pub const fn on_paired(self) -> bool {
        match self {
            Self::List | Self::Detail | Self::Chat | Self::Load => true,
            Self::Manage => false,
        }
    }
}

/// A model, named by its machine: the machine, and the model's id in that
/// machine's catalogue, which is never reused there.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct ModelRef {
    /// The machine whose catalogue `id` is in.
    pub machine: Machine,
    /// The model's catalogue id on that machine.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number"))]
    pub id: i64,
}

/// The longest DNS label, and so the longest name [`machine_name`] keeps.
const MAX_LABEL: usize = 63;

/// The display name in `raw`, a host name: its first DNS label, kept only
/// when that label is 1 to 63 ASCII letters, digits, `-` or `_`.
///
/// `Desk.local` is `Desk`. A label carrying anything else — a control
/// character, a `/`, a space, a non-ASCII letter — gives `None`, as does an
/// empty or over-long one, so a caller shows its own fallback instead of
/// text the other machine chose.
#[must_use]
pub fn machine_name(raw: &str) -> Option<String> {
    let label = raw.split('.').next().unwrap_or_default();
    let plain = (1..=MAX_LABEL).contains(&label.len())
        && label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    plain.then(|| label.to_owned())
}

/// What the paired machine is shown as when it has given no name, or none
/// [`machine_name`] keeps. Never its fingerprint, which is identity and is
/// not shown.
pub const UNNAMED_PAIRED: &str = "the paired machine";

#[cfg(test)]
#[path = "machine_tests.rs"]
mod machine_tests;
