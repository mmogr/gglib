//! `/draw`: the session's Draw button.
//!
//! The image tool is offered to the model only for the message typed after
//! `/draw`, as the page offers it only for a message sent with its Draw
//! button pressed. [`DrawSwitch`] is that switch: `/draw` asks the daemon
//! whether it can draw and arms it, and it is cleared after each send, so a
//! model never starts a render on its own. The message sent while it is
//! armed says `draw` on its turn ([`DrawSwitch::turn`]), and the daemon's
//! run then draws, once.
//!
//! `/draw <model>` chooses the image model the session's pictures are drawn
//! with, by its name or id among the daemon's image models, and arms the
//! next message; `/draw default` goes back to the daemon's default rule;
//! `/draw list` prints the models ([`DrawSwitch::say`]). The choice is held
//! for the session, by this switch and nowhere else: it goes on each turn
//! that draws as its `image_model`, and never to the chat model.
//!
//! The CLI never drives `sd-server` itself: the daemon's run draws, and
//! whether it can is the daemon's answer ([`ImageGenerator`]). A session
//! whose model is on another machine (`--remote`) cannot draw, and `/draw`
//! says why.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gglib_core::contracts::http::images::{DrawingAvailability, ImageModelRow};
use gglib_core::ports::ImageGenerationPort;
use gglib_core::services::drawing_availability;
use gglib_mcp::DrawWith;

use crate::daemon_client::{DaemonHandle, images::ImageGenerator};
use crate::target::Target;

/// Why a `--remote` session cannot draw: the chat's model is there, and the
/// picture would be drawn here.
const FAR: &str = "this chat's model is on another machine, so it cannot draw here; a chat kept \
                   on that machine draws with its image model";

/// A session's Draw switch.
#[derive(Debug)]
pub(crate) struct DrawSwitch {
    armed: Arc<AtomicBool>,
    /// The image model chosen for the session; none for the daemon's
    /// default rule. Shared with the session's image tool.
    model: DrawWith,
    /// What draws, or why nothing does.
    with: Result<Arc<dyn ImageGenerationPort>, String>,
}

impl DrawSwitch {
    /// The switch of a session on `target`, which asks `daemon` whether it
    /// can draw.
    pub(crate) fn for_session(daemon: &DaemonHandle, target: Target) -> Self {
        if target == Target::Remote {
            return Self::off(FAR);
        }
        Self::through(Arc::new(ImageGenerator::Daemon(daemon.clone())))
    }

    /// A switch that draws through `images`.
    pub(crate) fn through(images: Arc<dyn ImageGenerationPort>) -> Self {
        Self {
            armed: Arc::default(),
            model: DrawWith::default(),
            with: Ok(images),
        }
    }

    /// A switch that never arms, for `reason`.
    pub(crate) fn off(reason: &str) -> Self {
        Self {
            armed: Arc::default(),
            model: DrawWith::default(),
            with: Err(reason.to_owned()),
        }
    }

    /// A `/draw` line: with nothing after it, [`Self::arm`]; `list`, the
    /// daemon's image models; `default`, forget the session's choice and
    /// arm; anything else, choose the image model of that name or id and
    /// arm. What it says is the line to print.
    pub(crate) async fn say(&self, what: Option<&str>) -> String {
        match what.map(str::trim).filter(|what| !what.is_empty()) {
            None => self.arm().await,
            Some("list") => match self.ask(self.model.get()).await {
                Ok(answer) => listing(&answer, self.model.get()),
                Err(why) => why,
            },
            Some("default") => {
                self.model.set(None);
                self.arm().await
            }
            Some(named) => self.choose(named).await,
        }
    }

    /// `/draw`: arm the next message when the daemon can draw, with the
    /// session's image model when one was chosen, and say so; otherwise say
    /// why not, and arm nothing. A chosen model that can no longer draw is
    /// not swapped for the default: it stays chosen, and the line says how
    /// to change it.
    pub(crate) async fn arm(&self) -> String {
        let chosen = self.model.get();
        let answer = match self.ask(chosen).await {
            Ok(answer) => answer,
            Err(why) => return why,
        };
        if !answer.available {
            let reason = answer.reason.unwrap_or_default();
            return match chosen {
                Some(_) => format!(
                    "cannot draw with this session's image model: {reason}\n  /draw <model> \
                     chooses another, /draw default the daemon's default, /draw list shows them"
                ),
                None => format!("cannot draw: {reason}"),
            };
        }
        self.armed.store(true, Ordering::SeqCst);
        let model = answer.model.unwrap_or_default();
        let whose = if chosen.is_some() {
            ", this session's image model"
        } else {
            ""
        };
        format!("the next message draws, with {model}{whose}")
    }

    /// `/draw <model>`: choose the daemon's image model `named`, by id or by
    /// name (in full, or a part only one of them has), when the daemon says
    /// it can draw, and arm the next message. A name that fits none, or
    /// more than one, changes nothing and is answered with the list; so is
    /// a model that cannot draw, with why.
    async fn choose(&self, named: &str) -> String {
        let answer = match self.ask(None).await {
            Ok(answer) => answer,
            Err(why) => return why,
        };
        let held = self.model.get();
        let row = match find(&answer.models, named) {
            Ok(row) => row,
            Err(why) => return format!("{why}\n{}", listing(&answer, held)),
        };
        let judged = match self.ask(Some(row.id)).await {
            Ok(judged) => judged,
            Err(why) => return why,
        };
        if !judged.available {
            let reason = judged.reason.unwrap_or_default();
            return format!("cannot draw with {}: {reason}", row.name);
        }
        self.model.set(Some(row.id));
        self.armed.store(true, Ordering::SeqCst);
        format!(
            "the next message draws, with {}, this session's image model",
            row.name
        )
    }

    /// The daemon's answer about drawing with `chosen`, or the line that
    /// says a session like this one cannot draw at all.
    async fn ask(&self, chosen: Option<u32>) -> Result<DrawingAvailability, String> {
        match &self.with {
            Ok(images) => {
                Ok(drawing_availability(Some(images.as_ref()), false, None, chosen).await)
            }
            Err(reason) => Err(format!("cannot draw: {reason}")),
        }
    }

    /// The image model chosen for the session, if one is.
    #[cfg(test)]
    pub(crate) fn chosen(&self) -> Option<u32> {
        self.model.get()
    }

    /// What the next message's turn says of drawing: whether it draws, and
    /// with the session's image model, if one was chosen. It draws while the
    /// switch is armed, and never says a model when it does not.
    pub(crate) fn turn(&self) -> (bool, Option<u32>) {
        let draws = self.armed.load(Ordering::SeqCst);
        (draws, self.model.get().filter(|_| draws))
    }

    /// A message was sent and its turn is over: the switch is off again.
    pub(crate) fn sent(&self) {
        self.armed.store(false, Ordering::SeqCst);
    }

    /// Whether the next message may draw.
    #[cfg(test)]
    pub(crate) fn is_armed(&self) -> bool {
        self.armed.load(Ordering::SeqCst)
    }
}

/// The one of `rows` that `named` names: by its id, by its whole name, or by
/// a part of the name no other row has, whatever the case.
///
/// # Errors
///
/// The line to print: no row fits, or more than one does.
fn find<'a>(rows: &'a [ImageModelRow], named: &str) -> Result<&'a ImageModelRow, String> {
    if let Some(row) = named
        .parse::<u32>()
        .ok()
        .and_then(|id| rows.iter().find(|row| row.id == id))
    {
        return Ok(row);
    }
    let wanted = named.to_lowercase();
    if let Some(row) = rows.iter().find(|row| row.name.to_lowercase() == wanted) {
        return Ok(row);
    }
    let fits: Vec<&ImageModelRow> = rows
        .iter()
        .filter(|row| row.name.to_lowercase().contains(&wanted))
        .collect();
    match fits.as_slice() {
        [row] => Ok(row),
        [] => Err(format!(
            "no image model here is named or numbered `{named}`"
        )),
        several => Err(format!(
            "`{named}` fits {} image models; say more of the name, or its id",
            several.len()
        )),
    }
}

/// The daemon's image models, one to a line, in its order: `*` on the one
/// the next `/draw` message would draw with, then the id, the name, the
/// family, and what else is true of it (the daemon's default, loaded now,
/// the session's choice, or the roles it has no file for). Under them, why
/// nothing would draw, when nothing would.
fn listing(answer: &DrawingAvailability, chosen: Option<u32>) -> String {
    if answer.models.is_empty() {
        let why = answer.reason.as_deref().unwrap_or("none is installed");
        return format!("the daemon lists no image models: {why}");
    }
    let mut lines: Vec<String> = answer
        .models
        .iter()
        .map(|row| {
            let mark = if answer.model_id == Some(row.id) {
                '*'
            } else {
                ' '
            };
            let mut notes = vec![row.family.label().to_owned()];
            if row.default {
                notes.push("default".to_owned());
            }
            if row.resident {
                notes.push("loaded".to_owned());
            }
            if chosen == Some(row.id) {
                notes.push("this session's".to_owned());
            }
            if !row.complete {
                let roles: Vec<&str> = row.missing.iter().map(|role| role.label()).collect();
                notes.push(format!("incomplete, missing {}", roles.join(", ")));
            }
            format!("{mark} {:>4}  {}  ({})", row.id, row.name, notes.join(", "))
        })
        .collect();
    if !answer.available {
        lines.push(format!(
            "  nothing would draw: {}",
            answer.reason.as_deref().unwrap_or_default()
        ));
    }
    lines.push(
        "  /draw <name or id> chooses one for this session; /draw default forgets it".to_owned(),
    );
    lines.join("\n")
}

#[cfg(test)]
#[path = "draw_model_tests.rs"]
mod model_tests;
#[cfg(test)]
#[path = "draw_tests.rs"]
mod tests;
