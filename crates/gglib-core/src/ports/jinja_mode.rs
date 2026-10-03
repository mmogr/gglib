//! A launch's position on Jinja chat templating.

use serde::{Deserialize, Serialize};

/// What position a launch takes on Jinja chat templating.
///
/// Three states rather than a bool because llama-server's default is jinja
/// **on**: `use_jinja` initialises to `true` (`common/common.h:621`) and
/// `common/arg.cpp:1394-1399` flips it off only for the completion and mtmd
/// examples — never for the server. So "gglib emits no flag" and "gglib turns
/// jinja off" are two different launches, and a bool could only ever name one
/// of them. If `false` meant *emit nothing*, a user who explicitly disabled
/// Jinja would get a server running with it anyway, silently.
///
/// The distinction is in the type rather than in a convention because both
/// falsy cases are reachable and they must not be conflated — see
/// [`Self::Defer`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JinjaMode {
    /// Emit no jinja flag at all and let llama-server decide.
    ///
    /// The default, and what an untagged model with no override resolves to.
    /// Against this pinned llama.cpp that means jinja is **on** — deferring is
    /// not the same as turning it off, and gglib does not pretend otherwise.
    #[default]
    Defer,
    /// Emit `--jinja`.
    On,
    /// Emit `--no-jinja`.
    ///
    /// Reached only from an explicit caller override. Nothing tag-derived
    /// produces this: taking jinja away removes tool-call templating and
    /// template kwargs, which is a decision only the user gets to make.
    Off,
}
