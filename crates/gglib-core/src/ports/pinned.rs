//! [`PinnedSpec`]: the one model a runtime will serve, and how to launch it.

use crate::server_config::ServerConfigOptions;
// Named only by the doc links below.
#[cfg(doc)]
use super::ModelRuntimePort;

/// A runtime pin: the one model a runtime will serve, plus how to launch it.
///
/// Carried by [`ModelRuntimePort::set_pin`] and serialized inside the
/// daemon's `POST /api/proxy/start` body, which is why it derives serde.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct PinnedSpec {
    /// Name clients must address the model by. Matched exactly.
    pub name: String,
    /// Standing launch options for the pinned model, already resolved
    /// through the caller's cascade — layered onto the runtime's template at
    /// launch, winning field-wise (the cascade has already run; the template
    /// must not undo it).
    #[serde(default)]
    pub launch_overrides: ServerConfigOptions,
}
