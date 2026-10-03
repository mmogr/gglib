//! A turn's model, resolved once on the machine that serves it.
//!
//! A `#[path]` child of `target.rs`. Locally the identifier is looked up in
//! this catalogue; on the paired machine it is sent to that machine's detail
//! route through the daemon, which resolves it as it resolves a turn's —
//! catalogue id first, then exact name, and a `:profile` suffix routed to a
//! profile configured *there* — and answers with the model and the profile.
//! Either way the answer is a [`TurnModel`]: the model named by its machine,
//! the name it is shown by, and, on the paired machine, the `<id>` or
//! `<id>:<profile>` the turn then puts on the wire. That one answer is what
//! the banner names, what the conversation stores and what is remembered on
//! the pairing, so a name that two models share cannot send the turn to one
//! and record the other, and a model the far machine does not have is
//! refused before a turn starts.

use anyhow::{Context as _, Result, anyhow, bail};
use gglib_app_services::far_credentials;
use gglib_core::Settings;
use gglib_core::domain::{Machine, Model, ModelLookup, ModelRef};
use gglib_core::ports::SettingsRepository;

use super::Target;
use crate::bootstrap::CliContext;

/// A turn's model, as the machine that serves it resolved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TurnModel {
    /// What the turn names the model by. Here, the identifier as it was
    /// typed, remembered or replayed. On the paired machine, the model's id
    /// there with the profile it named, `<id>` or `<id>:<profile>`: the
    /// name that goes on the wire.
    pub identifier: String,
    /// The model, named by its machine. `None` for an identifier this
    /// catalogue does not hold, which only a server named with `--port`
    /// answers to.
    pub model_ref: Option<ModelRef>,
    /// The name the model goes by on its machine, as it is shown and saved;
    /// the identifier when it did not resolve.
    pub name: String,
    /// The profile the identifier named on the paired machine, which that
    /// machine applies. A profile here is `profile_selection`'s.
    pub far_profile: Option<String>,
    /// The name the paired machine is shown by; `None` for this one.
    pub machine: Option<String>,
}

impl TurnModel {
    /// A turn on this machine, with the catalogue entry `identifier` names,
    /// if any.
    pub(crate) fn here(identifier: String, model: Option<Model>) -> Self {
        Self {
            name: model
                .as_ref()
                .map_or_else(|| identifier.clone(), |m| m.name.clone()),
            model_ref: model.map(|m| ModelRef {
                machine: Machine::Local,
                id: m.id,
            }),
            identifier,
            far_profile: None,
            machine: None,
        }
    }

    /// A turn on the paired machine, as it answered the lookup: the machine
    /// it is (`fingerprint`, never shown) and the name it is shown by.
    pub(crate) fn far(lookup: ModelLookup, fingerprint: String, machine: String) -> Self {
        let id = lookup.detail.id;
        Self {
            identifier: far_wire(id, lookup.profile.as_deref()),
            model_ref: Some(ModelRef {
                machine: Machine::Paired { fingerprint },
                id,
            }),
            name: lookup.detail.name,
            far_profile: lookup.profile,
            machine: Some(machine),
        }
    }

    /// The model as a person reads it, in forms they can type: `qwen3 (3) on
    /// desk`, or `qwen3 (3:coding) on desk` with a profile.
    pub(crate) fn shown(&self) -> String {
        self.machine.as_ref().map_or_else(
            || self.name.clone(),
            |machine| format!("{} ({}) on {machine}", self.name, self.identifier),
        )
    }
}

/// The paired machine's model `id`, with the profile configured there, as
/// a turn names it on the wire: `3`, or `3:coding`. That machine's proxy
/// routes the suffix to its profile, as it does a name's.
pub(crate) fn far_wire(id: i64, profile: Option<&str>) -> String {
    profile.map_or_else(|| id.to_string(), |profile| format!("{id}:{profile}"))
}

impl Target {
    /// The model a turn is for, resolved once on the machine that serves it.
    ///
    /// Locally, the catalogue entry `identifier` names, when there is one. On
    /// the paired machine, what its detail route answers for `identifier`,
    /// remembered on the pairing in the form the turn sends.
    ///
    /// # Errors
    ///
    /// On the paired machine, a daemon that is not running or not
    /// connected, a model that machine does not have — before any turn
    /// starts — and a settings write that failed.
    pub(crate) async fn resolve_turn(
        self,
        ctx: &CliContext,
        identifier: String,
    ) -> Result<TurnModel> {
        match self {
            Self::Local => {
                let model = self.local_model(ctx, &identifier).await;
                Ok(TurnModel::here(identifier, model))
            }
            Self::Remote => {
                let paired = self.paired(ctx).await?;
                let lookup = async |id: &str| paired.handle.paired_model(id).await;
                let turn = resolve_far(
                    &identifier,
                    paired.connection.ticket_fingerprint.clone(),
                    paired.name.clone(),
                    lookup,
                )
                .await?;
                let fingerprint = &paired.connection.ticket_fingerprint;
                remember(ctx.settings_repo.as_ref(), fingerprint, &turn.identifier).await?;
                Ok(turn)
            }
        }
    }
}

/// `identifier` on the paired machine, as `lookup` answers for it there.
pub(super) async fn resolve_far(
    identifier: &str,
    fingerprint: String,
    machine: String,
    lookup: impl AsyncFnOnce(&str) -> Result<ModelLookup>,
) -> Result<TurnModel> {
    let found = lookup(identifier)
        .await
        .with_context(|| format!("looking up '{identifier}' on {machine}"))?;
    Ok(TurnModel::far(found, fingerprint, machine))
}

/// The paired machine's model for this turn: `typed`, or what was
/// remembered; or a refusal that says how to find one. Writes nothing: the
/// turn's model is remembered once that machine has resolved it.
pub(super) async fn remembered_model(
    settings: &dyn SettingsRepository,
    typed: String,
) -> Result<String> {
    let stored = settings
        .load()
        .await
        .map_err(|e| anyhow!("failed to load settings: {e}"))?;
    let Some(pairing) = stored.remote_pairing else {
        bail!(
            "this machine has not paired with a remote — `gglib remote join <ticket>-<code>` \
             first"
        );
    };
    if typed.is_empty() {
        return pairing.default_model.ok_or_else(|| {
            anyhow!(
                "name a model the first time — `gglib model list --remote` shows the ones that \
                 machine serves; after that, --remote remembers the one you used"
            )
        });
    }
    Ok(typed)
}

/// Remember `wire`, a turn's resolved `<id>[:<profile>]`, on the stored
/// pairing for the next turn that names none, when that pairing is with
/// `fingerprint`, the machine that resolved it.
pub(super) async fn remember(
    settings: &dyn SettingsRepository,
    fingerprint: &str,
    wire: &str,
) -> Result<()> {
    let stored = settings
        .load()
        .await
        .map_err(|e| anyhow!("failed to load settings: {e}"))?;
    let Some(pairing) = stored
        .remote_pairing
        .filter(|pairing| far_credentials(Some(pairing), fingerprint).is_ok())
    else {
        return Ok(());
    };
    if pairing.default_model.as_deref() == Some(wire) {
        return Ok(());
    }
    settings
        .modify(&|now: &mut Settings| {
            remember_model(now, fingerprint, wire);
            Ok(())
        })
        .await
        .map(|_| ())
        .map_err(|e| anyhow!("could not remember the model for that machine: {e}"))
}

/// Write `model` into the stored pairing, and no other field of it, when
/// that pairing is with the machine `fingerprint` names, the one that
/// resolved it.
///
/// Applied to the settings as they stand when the write lands, because the
/// daemon writes the same record: one rebuilt from an earlier read would
/// put back the key a re-pair replaced, or a pairing since cleared. Checked
/// against the machine that resolved the model, not against a pairing read
/// after it answered, because a model named for one machine means nothing
/// on another.
fn remember_model(settings: &mut Settings, fingerprint: &str, model: &str) {
    if let Some(pairing) = settings
        .remote_pairing
        .as_mut()
        .filter(|pairing| far_credentials(Some(&**pairing), fingerprint).is_ok())
    {
        pairing.default_model = Some(model.to_owned());
    }
}

#[cfg(test)]
#[path = "target_turn_tests.rs"]
mod target_turn_tests;
