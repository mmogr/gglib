//! What `gglib model explain` prints and `GET /api/models/{id}/explain`
//! serves: the launch gglib would make for a model, and its sampling.
//!
//! The launch comes from the runtime's plan of it (`ModelRuntimePort::plan`),
//! over the same cascade a start runs (`plan_bare_launch`); the sampling
//! from `sampling_explain`. Nothing here resolves a value of its own.

use gglib_core::domain::{CommandLine, LaunchPlan};
use serde::{Deserialize, Serialize};

use crate::SamplingExplanationDto;

/// The launch gglib would make for a model now, and how it would sample.
///
/// Consumers:
/// - Axum: `GET /api/models/:id/explain`
/// - CLI: `gglib model explain`
/// - GUI frontend: the inspector
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct LaunchExplanationDto {
    /// The program and its arguments, each with the decision behind it,
    /// the narration a launch prints, and the context fit.
    pub plan: LaunchPlan,
    /// The plan's command line, written for the shell of the machine that
    /// made the plan ([`HostOs::shell`]): what every surface shows and
    /// copies, so none quotes a line itself.
    ///
    /// Sent whenever the plan's engine is installed. With none installed
    /// there is nothing to paste, and the plan's `not_installed` says why
    /// in the line's place. Otherwise `None` is the answer of a service
    /// older than this field, which a newer CLI or desktop app can meet
    /// while it runs: each says the service does not give the line, rather
    /// than quote one.
    ///
    /// [`HostOs::shell`]: gglib_core::host::HostOs::shell
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub command: Option<CommandLine>,
    /// The sampling the model resolves to, and where each value came from.
    pub sampling: SamplingExplanationDto,
}
