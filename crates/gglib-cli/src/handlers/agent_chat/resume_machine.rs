//! Which machine a resumed conversation runs on, and what it saves.
//!
//! A `#[path]` child of `resume_settings.rs`. A conversation that stored its
//! model goes back to that model's machine, by its id there, whatever the
//! flag says; one that names no model, or a model named on the command line,
//! follows the flag. The machine is read as the daemon reads it
//! (`Conversation::machine`), so a row that stores only a `model_id` ran
//! here: it refuses `--remote`, and resumes by the name it saved. A resume
//! whose turn is on another model than the one stored saves that model, so
//! the conversation names the model it last ran on.

use anyhow::{Result, bail};
use gglib_app_services::far_credentials;
use gglib_core::RemotePairing;
use gglib_core::domain::chat::{Conversation, ConversationSettings};
use gglib_core::domain::{InferenceProfile, Machine, UNNAMED_PAIRED};

use super::named_over;
use crate::bootstrap::CliContext;
use crate::handlers::inference::chat::ChatArgs;
use crate::target::{Target, TurnModel, far_wire};

/// The settings a resumed session saves in place of `held`: what the
/// conversation saved, with what this command line names in its place
/// ([`named_over`]), and, when its turn is on another model than the one
/// stored, that model, its name and the session's profile. `None` when that
/// changes nothing.
///
/// # Errors
///
/// A conversation whose stored model, the one it resumes with, is no longer
/// in this library: said here, rather than as a miss that sends the user to
/// the paired machine this chat never ran on.
pub(crate) async fn resumed_settings(
    ctx: &CliContext,
    held: &ConversationSettings,
    args: &ChatArgs,
    typed: bool,
    profile: Option<&InferenceProfile>,
    turn: &TurnModel,
) -> Result<Option<ConversationSettings>> {
    let stored = held.model.as_ref();
    if let Some(model) = stored.filter(|_| !typed)
        && turn.model_ref.is_none()
    {
        bail!(
            "this chat's model, {} ({}), is no longer in this library; name one to continue it \
             with: gglib chat <model> --continue {}",
            held.model_name.as_deref().unwrap_or("--"),
            model.id,
            args.continue_id.unwrap_or_default()
        );
    }
    super::require_a_model(ctx, turn).await?;
    let mut kept = named_over(held.clone(), args, profile, turn);
    if held.model.as_ref() != turn.model_ref.as_ref() {
        kept.model.clone_from(&turn.model_ref);
        kept.model_name = Some(turn.name.clone());
        kept.profile = super::session_settings(args, profile, turn).profile;
    }
    Ok((kept != *held).then_some(kept))
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
    stored: &Conversation,
    pairing: Option<&RemotePairing>,
) -> Result<()> {
    let named = !args.identifier.is_empty();
    let paired_with = |fingerprint: &str| far_credentials(pairing, fingerprint).is_ok();
    let Some((target, identifier)) = stored_machine(stored, args.target, named, paired_with)?
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
/// `None` when the flag decides: a row that names no model, or a model
/// named on the command line. And for a row that stores only a `model_id`,
/// once it is not refused: it ran here, and resumes by the name it saved.
/// `paired_with` says whether
/// the pairing this machine holds now is with the machine a fingerprint
/// names.
///
/// # Errors
///
/// A conversation that ran here, resumed with `--remote`; one that ran on a
/// machine this one is no longer paired with. One sentence each.
pub(crate) fn stored_machine(
    stored: &Conversation,
    flag: Target,
    named: bool,
    paired_with: impl Fn(&str) -> bool,
) -> Result<Option<(Target, String)>> {
    let Some(machine) = stored.machine().filter(|_| !named) else {
        return Ok(None);
    };
    let saved = stored.settings.as_ref();
    let model = saved.and_then(|saved| saved.model.as_ref());
    let resumed = match &machine {
        Machine::Local if flag == Target::Remote => {
            bail!("this chat ran on this machine and continues here; drop --remote to resume it")
        }
        Machine::Local => model.map(|model| (Target::Local, model.id.to_string())),
        Machine::Paired { fingerprint } if !paired_with(fingerprint) => bail!(
            "this chat ran on a machine this one is no longer paired with, so it cannot be \
             resumed here"
        ),
        Machine::Paired { .. } => model.map(|model| {
            let profile = saved.and_then(|saved| saved.profile.as_deref());
            (Target::Remote, far_wire(model.id, profile))
        }),
    };
    Ok(resumed)
}

#[cfg(test)]
#[path = "resume_machine_tests.rs"]
mod resume_machine_tests;
