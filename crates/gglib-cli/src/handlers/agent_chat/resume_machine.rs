//! Which machine a resumed conversation runs on, and what it saves.
//!
//! A `#[path]` child of `resume_settings.rs`. A conversation that stored its
//! model goes back to that model's machine, by its id there, whatever the
//! flag says; one that stored none, or a model named on the command line,
//! follows the flag. A resume whose turn is on another model than the one
//! stored saves that model, so the conversation names the model it last ran
//! on.

use anyhow::{Result, bail};
use gglib_app_services::far_credentials;
use gglib_core::RemotePairing;
use gglib_core::domain::chat::ConversationSettings;
use gglib_core::domain::{InferenceProfile, Machine, UNNAMED_PAIRED};

use super::session_profile;
use crate::handlers::inference::chat::ChatArgs;
use crate::target::{Target, TurnModel, far_wire};

/// The settings a resumed session saves in place of `saved`, when its turn
/// is on another model than the one stored: that model, its name and the
/// session's profile, with every other setting kept. `None` when the stored
/// model is the turn's, whose profile `--profile` replaces for this session
/// only.
///
/// # Errors
///
/// A conversation whose stored model, the one it resumes with, is no longer
/// in this library and no `--port` serves: said here, rather than as a miss
/// that sends the user to the paired machine this chat never ran on.
pub(crate) fn resumed_settings(
    saved: Option<ConversationSettings>,
    args: &ChatArgs,
    typed: bool,
    profile: Option<&InferenceProfile>,
    turn: &TurnModel,
) -> Result<Option<ConversationSettings>> {
    let stored = saved.as_ref().and_then(|s| s.model.as_ref());
    if let Some(model) = stored.filter(|_| !typed && args.port.is_none())
        && turn.model_ref.is_none()
    {
        bail!(
            "this chat's model, {} ({}), is no longer in this library; name one to continue it \
             with: gglib chat <model> --continue {}",
            saved
                .as_ref()
                .and_then(|s| s.model_name.as_deref())
                .unwrap_or("--"),
            model.id,
            args.continue_id.unwrap_or_default()
        );
    }
    if stored == turn.model_ref.as_ref() {
        return Ok(None);
    }
    let mut kept = saved.unwrap_or_default();
    kept.model.clone_from(&turn.model_ref);
    kept.model_name = Some(turn.name.clone());
    kept.profile = session_profile(profile, turn);
    Ok(Some(kept))
}

/// [`stored_machine`] applied to a resume's `args`, against the pairing
/// this machine holds now. A conversation that goes to the paired machine
/// without `--remote` says so, naming it.
///
/// # Errors
///
/// [`stored_machine`]'s refusals.
pub(crate) fn follow_stored_machine(
    args: &mut ChatArgs,
    saved: Option<&ConversationSettings>,
    pairing: Option<&RemotePairing>,
) -> Result<()> {
    let named = !args.identifier.is_empty();
    let paired_with = |fingerprint: &str| far_credentials(pairing, fingerprint).is_ok();
    let port = args.port.is_some();
    let Some((target, identifier)) = stored_machine(saved, args.target, port, named, paired_with)?
    else {
        return Ok(());
    };
    if target == Target::Remote && args.target == Target::Local {
        let name = pairing.and_then(|p| p.name.as_deref());
        eprintln!(
            "  This chat ran on {}, so it continues there.",
            name.unwrap_or(UNNAMED_PAIRED)
        );
    }
    args.target = target;
    args.identifier = identifier;
    Ok(())
}

/// Where a resumed conversation runs, and the identifier it resumes with,
/// when the conversation decides: it stored its model, and this command
/// line names none. Then the model's machine decides, not the flag, and the
/// model is named by its id there — with its profile, on the paired machine,
/// where the profile is part of what goes on the wire.
///
/// `None` when the flag decides: a row that stores no model, or a model
/// named on the command line. `port` says whether `--port` names a server
/// here; `paired_with`, whether the pairing this machine holds now is with
/// the machine a fingerprint names.
///
/// # Errors
///
/// A conversation that ran here, resumed with `--remote`; one that ran on a
/// machine this one is no longer paired with; one that ran on the paired
/// machine, resumed with `--port`. One sentence each.
pub(crate) fn stored_machine(
    saved: Option<&ConversationSettings>,
    flag: Target,
    port: bool,
    named: bool,
    paired_with: impl Fn(&str) -> bool,
) -> Result<Option<(Target, String)>> {
    let Some((model, saved)) = saved
        .filter(|_| !named)
        .and_then(|saved| Some((saved.model.as_ref()?, saved)))
    else {
        return Ok(None);
    };
    match &model.machine {
        Machine::Local if flag == Target::Remote => {
            bail!("this chat ran on this machine and continues here; drop --remote to resume it")
        }
        Machine::Local => Ok(Some((Target::Local, model.id.to_string()))),
        Machine::Paired { fingerprint } if !paired_with(fingerprint) => bail!(
            "this chat ran on a machine this one is no longer paired with, so it cannot be \
             resumed here"
        ),
        Machine::Paired { .. } if port => bail!(
            "this chat ran on the paired machine, and --port names a server on this one; drop \
             --port to resume it there"
        ),
        Machine::Paired { .. } => Ok(Some((
            Target::Remote,
            far_wire(model.id, saved.profile.as_deref()),
        ))),
    }
}

#[cfg(test)]
#[path = "resume_machine_tests.rs"]
mod resume_machine_tests;
