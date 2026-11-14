//! What a gglib says of itself on its `/v1/models` list: its version, and
//! which of gglib's own features this process serves.
//!
//! A `#[path]` child of `models.rs`, which re-exports both types.
//! `models_endpoint.rs` fills it in from what the serving process was given,
//! so the desktop daemon's proxy, the phone's engine and a bare test proxy
//! each say only what they really answer. Another gglib reads it to know
//! what to ask for before it asks.
//!
//! These names are fixed: a field or feature is only ever added, and a
//! reader ignores one it does not know (`docs/compatibility.md`).

use serde::{Deserialize, Serialize};

/// The `gglib` object on a `/v1/models` answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
pub struct GglibInfo {
    /// The build's version: gglib's, as its release names it.
    pub version: String,
    /// Which of gglib's own features this process serves, by
    /// [`Feature::name`]. Never left out: an empty list serves none of them.
    /// A reader ignores a name it does not know.
    pub features: Vec<String>,
}

impl GglibInfo {
    /// This build, serving `features`.
    #[must_use]
    pub fn this_build(features: impl IntoIterator<Item = Feature>) -> Self {
        Self {
            version: THIS_VERSION.to_owned(),
            features: features.into_iter().map(|f| f.name().to_owned()).collect(),
        }
    }

    /// Whether `feature` is among those served.
    #[must_use]
    pub fn serves(&self, feature: Feature) -> bool {
        self.features.iter().any(|f| f == feature.name())
    }
}

/// This build's version: the workspace's, which is gglib's.
pub const THIS_VERSION: &str = env!("CARGO_PKG_VERSION");

/// One of gglib's own features a process may serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// `chats`: the hub's chats and their images (`/v1/gglib/chats`,
    /// `/v1/gglib/attachments`), and a turn added to one.
    Chats,
    /// `runs`: a device's runs (`/v1/gglib/runs`).
    Runs,
    /// `drawing`: whether this machine can draw, and its image models
    /// (`/v1/gglib/images/drawing`), and a render.
    Drawing,
    /// `status`: the proxy's status (`/v1/gglib/proxy/status` and its stream).
    Status,
}

impl Feature {
    /// Every feature, in the order a list names them.
    pub const ALL: [Self; 4] = [Self::Chats, Self::Runs, Self::Drawing, Self::Status];

    /// The feature's name in [`GglibInfo::features`].
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Chats => "chats",
            Self::Runs => "runs",
            Self::Drawing => "drawing",
            Self::Status => "status",
        }
    }
}
