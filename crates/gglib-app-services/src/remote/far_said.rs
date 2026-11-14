//! What the far machine says of itself, and whether it serves a call before
//! the call is made.
//!
//! A `#[path]` child of `far_proxy.rs`. A gglib says its version and the
//! features it serves in the `gglib` object of its `/v1/models` answer
//! (`gglib_proxy::models::GglibInfo`). A list with no such object is a gglib
//! from before devices said their version, and each of gglib's own calls to
//! it is refused with one sentence asking for that machine to be updated. A
//! list that does not name a feature refuses the calls that need it. Nothing
//! is shimmed and no version is compared: what the far machine lists is the
//! whole answer.
//!
//! The answer is read once each connect (`paired_machine.rs`), and again
//! whenever the model list is read, and kept with the connection ([`Said`]).
//! Only an answer that serves the call lets it go unasked: one that does not,
//! or none, is read again first, so a machine updated while this one stayed
//! connected is served at once. A read that gets no answer refuses nothing;
//! the call goes ahead and fails in its own words.

use std::sync::{Arc, Mutex, PoisonError};

use gglib_proxy::models::{Feature, GglibInfo, THIS_VERSION};
use serde::Deserialize;

use super::FarProxy;
use super::far_read::FarError;
use crate::error::GuiError;

/// The far machine's last answer about itself, shared by every [`FarProxy`]
/// built for one connection. `None` until one is read, and while the last one
/// read had no `gglib` object.
#[derive(Debug, Clone, Default)]
pub(crate) struct Said(Arc<Mutex<Option<GglibInfo>>>);

impl Said {
    pub(super) fn keep(&self, gglib: Option<GglibInfo>) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = gglib;
    }

    fn kept(&self) -> Option<GglibInfo> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The parts of a `/v1/models` answer that say who answered, read without
/// its models, so a gglib whose models this build cannot read is still told
/// apart.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct About {
    /// That machine's name, as it gave it.
    pub(crate) machine_name: Option<String>,
    /// What it said of itself; `None` from a gglib from before devices said
    /// their version.
    pub(crate) gglib: Option<GglibInfo>,
}

impl FarProxy {
    /// Go ahead with one of gglib's own calls, which needs every one of
    /// `features` served there: at once when the kept answer serves them,
    /// else after reading that machine's answer again.
    ///
    /// # Errors
    ///
    /// `Conflict`, asking for an update, when that machine runs a gglib from
    /// before devices said their version; `Conflict`, saying what it does not
    /// serve, when its list does not name one of `features`.
    pub(super) async fn offers(&self, features: &[Feature]) -> Result<(), GuiError> {
        let serves = |gglib: &GglibInfo| features.iter().all(|&f| gglib.serves(f));
        if self.said.kept().is_some_and(|gglib| serves(&gglib)) {
            return Ok(());
        }
        match self.about().await {
            Ok(About {
                gglib: Some(gglib), ..
            }) => match features.iter().find(|&&f| !gglib.serves(f)) {
                Some(&missing) => Err(self.lacks(&gglib, missing)),
                None => Ok(()),
            },
            Ok(About { gglib: None, .. }) => Err(self.older()),
            Err(e) => {
                tracing::debug!(error = ?e, "could not read what the far machine serves");
                Ok(())
            }
        }
    }

    /// `GET /v1/models`, read for who answered alone, which is kept.
    ///
    /// # Errors
    ///
    /// As [`FarProxy::models`], but for a gglib from before devices said
    /// their version, whose answer this reads.
    pub(crate) async fn about(&self) -> Result<About, FarError> {
        Ok(self.listed().await?.1)
    }

    /// What a far machine on a gglib from before devices said their version
    /// is told, by the name it is shown by.
    pub(super) fn older(&self) -> GuiError {
        let name = self.shown_name();
        GuiError::Conflict(format!(
            "{name} runs a gglib older than this machine's {THIS_VERSION} — update gglib on {name}"
        ))
    }

    /// What is said of a call that needs `missing`, which that machine does
    /// not serve.
    fn lacks(&self, gglib: &GglibInfo, missing: Feature) -> GuiError {
        let not = match missing {
            Feature::Chats => "does not share its chats with this machine",
            Feature::Runs => "does not share its runs with this machine",
            Feature::Drawing => "does not draw for this machine",
            Feature::Status => "does not share its status with this machine",
        };
        GuiError::Conflict(format!(
            "{}, on gglib {}, {not}",
            self.shown_name(),
            gglib.version
        ))
    }

    /// The list's body and who answered it, which is kept.
    pub(super) async fn listed(&self) -> Result<(Vec<u8>, About), FarError> {
        let body = self.list_body().await?;
        let about: About =
            serde_json::from_slice(&body).map_err(|e| super::far_read::unreadable(&e))?;
        self.said.keep(about.gglib.clone());
        Ok((body, about))
    }
}

#[cfg(test)]
#[path = "far_said_tests.rs"]
mod far_said_tests;
