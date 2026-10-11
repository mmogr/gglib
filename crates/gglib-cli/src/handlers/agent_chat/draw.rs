//! `/draw`: the session's Draw button.
//!
//! The image tool is offered to the model only for the message typed after
//! `/draw`, as the page offers it only for a message sent with its Draw
//! button pressed. [`DrawSwitch`] is that switch: `/draw` asks the daemon
//! whether it can draw and arms it, and it is cleared after each send, so a
//! model never starts a render on its own. The message sent while it is
//! armed does draw, once: its run's first reply must be the call for the
//! picture, and the reply after it is offered no tools
//! ([`DrawSwitch::first_call`]).
//!
//! `/draw <model>` chooses the image model the session's pictures are drawn
//! with, by its name or id among the daemon's image models, and arms the
//! next message; `/draw default` goes back to the daemon's default rule;
//! `/draw list` prints the models ([`DrawSwitch::say`]). The choice is held
//! for the session, by this switch and nowhere else: it goes on each request
//! the image tool hands the daemon, and never to the chat model.
//!
//! The CLI never drives `sd-server` itself. The tool draws through the
//! daemon ([`DaemonImageGenerator`]), so a render queues with the daemon's
//! other work; a session with no daemon, or one whose model is on another
//! machine (`--remote`), cannot draw, and `/draw` says why.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gglib_core::contracts::http::images::{DrawingAvailability, ImageModelRow};
use gglib_core::domain::agent::FirstCall;
use gglib_core::ports::ImageGenerationPort;
use gglib_core::services::{AttachmentService, drawing_availability};
use gglib_mcp::{DrawArm, DrawWith, DrawingTool};

use crate::bootstrap::CliContext;
use crate::daemon_client::{self, DaemonHandle, images::DaemonImageGenerator};
use crate::target::Target;

/// Why a `--remote` session cannot draw: its loop would run the tool here,
/// while the chat's model is there.
const FAR: &str = "this chat's model is on another machine, so it cannot draw here; a chat kept \
                   on that machine draws with its image model";

/// Why a `--port` session with no daemon cannot draw.
const NO_DAEMON: &str = "no gglib daemon is running to draw with; start one with `gglib daemon \
                         run`, then type /draw again";

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
    /// The switch of a session on `target`, reusing a llama-server on `port`
    /// when one is named. A session the daemon starts a model for draws
    /// through that daemon; one on `--port` only through a daemon already
    /// running, and none is started for it.
    pub(crate) async fn for_session(ctx: &CliContext, target: Target, port: Option<u16>) -> Self {
        if target == Target::Remote {
            return Self::off(FAR);
        }
        // A test that names no stand-in daemon never reaches this machine's.
        #[cfg(test)]
        if daemon_client::STAND_IN_PORT.try_with(|_| ()).is_err() {
            return Self::off(NO_DAEMON);
        }
        let daemon = if port.is_some() {
            match daemon_client::running(ctx).await {
                Ok(daemon) => daemon,
                Err(_) => return Self::off(NO_DAEMON),
            }
        } else {
            DaemonHandle::new(ctx, gglib_proxy::loopback::client()).await
        };
        Self::through(Arc::new(DaemonImageGenerator::new(daemon)))
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

    /// The drawing tool this session's loop is composed with, armed only
    /// while this switch is; none for a session that cannot draw.
    pub(crate) fn tool(&self, ctx: &CliContext) -> Option<(DrawingTool, DrawArm)> {
        let images = self.with.as_ref().ok()?;
        let store = AttachmentService::new(ctx.app.attachments().store());
        Some((
            DrawingTool::new(Arc::clone(images), Arc::new(store)).drawing_with(self.model.clone()),
            DrawArm::Shared(Arc::clone(&self.armed)),
        ))
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

    /// What the next message's run is held to while the switch is armed:
    /// its first reply calls the image tool and its second is offered no
    /// tools, as with a run sent with the page's Draw button. Nothing while
    /// it is off.
    pub(crate) fn first_call(&self) -> Option<FirstCall> {
        self.armed
            .load(Ordering::SeqCst)
            .then(DrawingTool::first_call)
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
