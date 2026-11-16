//! Why a model did not load, in the one sentence every surface shows.
//!
//! The runtime fills a [`LoadFailure`] where a load fails; this file is the
//! only place the words are made, in two sentences. The whole one
//! ([`LoadFailure::sentence`], its `Display`) is this machine's: the
//! daemon's API sends it as the error of a coded refusal
//! ([`LoadFailure::CODE`]), so the CLI prints it and the app places it as it
//! stands, and this machine's own turns end with it (#1959 C5). Anything
//! that can be another machine, the endpoint's `OpenAI` clients and a
//! paired device's turn, is told [`LoadFailure::remote_sentence`]: the kind
//! of failure, and nothing the engine printed or this machine's paths.

use std::fmt;

use crate::domain::RuntimeKind;
use crate::utils::process::bounded::Failure;

/// A model's load that failed, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadFailure {
    /// The model's name.
    pub model: String,
    /// The engine that was to serve it.
    pub runtime: RuntimeKind,
    /// Why it did not load.
    pub cause: LoadCause,
    /// The context gglib fitted to this machine for the load, when it chose
    /// one: a server that failed with it may have failed for its size.
    pub fitted_context: Option<u64>,
}

/// Why a load failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadCause {
    /// The engine cannot start: its check found it so, or starting it
    /// failed for the binary's sake (not there, not a program, refused by
    /// the system). Nothing runs.
    EngineNotWorking {
        /// How it failed.
        failure: Failure,
        /// The check's sentence, as the engine's status gives it.
        reason: String,
    },
    /// The server was not started, for a reason that is not its binary's.
    NotStarted {
        /// Why, as the process core said it.
        reason: String,
    },
    /// The server was started, and another program answers on its port.
    PortTaken {
        /// The port.
        port: u16,
    },
    /// The server ended before it was ready.
    Stopped {
        /// How it ended, read by
        /// [`failure_kind`](crate::utils::process::bounded::failure_kind);
        /// `None` when it exited 0.
        failure: Option<Failure>,
        /// The line of its output that says why: see the runtime's rule.
        said: Option<String>,
    },
    /// The server was still not ready when its wait was up.
    NotReady {
        /// How long it was given.
        waited_secs: u64,
        /// The line of its output that says where it was.
        said: Option<String>,
    },
}

impl LoadFailure {
    /// The code a failed load is refused with by the daemon's API (as its
    /// error's `type`) and by the endpoint (as its error's `code`). A turn
    /// whose model did not load ends with `model_unavailable` instead, and
    /// the `server_error` event carries no code.
    pub const CODE: &'static str = "model_load_failed";

    /// The whole sentence, for this machine's own surfaces: the CLI, the
    /// app and the page over the daemon's API, and this machine's own
    /// turns. It is the `Display`.
    #[must_use]
    pub fn sentence(&self) -> String {
        self.to_string()
    }

    /// The sentence anything that can be another machine is told: the
    /// endpoint's clients, and a paired device's turn. The kind of failure
    /// in words, and nothing this machine holds: the engine's printed line
    /// and the check's reason are left out, as they can name a file, a
    /// folder or a library's path here; so is the fitted context, which
    /// only this machine's settings can change.
    #[must_use]
    pub fn remote_sentence(&self) -> String {
        let server = self.runtime.server_name();
        let model = &self.model;
        // A library is the one cause said, in words of its kind, never by
        // its name or path.
        let library = |failure: Option<&Failure>| match failure {
            Some(Failure::MissingLibrary { .. }) => ": a library it needs is missing",
            Some(Failure::BadLibrary { .. }) => ": a library it needs could not be loaded",
            _ => "",
        };
        match &self.cause {
            LoadCause::EngineNotWorking {
                failure: Failure::NotFound,
                ..
            } => format!("{server} is not installed on the hub, so {model} could not be loaded"),
            LoadCause::EngineNotWorking { failure, .. } => {
                format!("{server} cannot start on the hub{}", library(Some(failure)))
            }
            LoadCause::NotStarted { .. } => {
                format!("{server} could not be started for {model} on the hub")
            }
            LoadCause::PortTaken { .. } => format!(
                "{server} could not be reached for {model} on the hub: another program holds \
                 its port"
            ),
            LoadCause::Stopped { failure, .. } => format!(
                "{server} stopped while loading {model} on the hub{}",
                library(failure.as_ref())
            ),
            LoadCause::NotReady { waited_secs, .. } => format!(
                "{server} was not ready within {waited_secs} s of starting to load {model} on the hub"
            ),
        }
    }
}

impl std::error::Error for LoadFailure {}

impl fmt::Display for LoadFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { model, cause, .. } = self;
        let server = self.runtime.server_name();
        match cause {
            // The check's sentence names the engine and the remedy, and no
            // server ran, so no context is blamed.
            LoadCause::EngineNotWorking { reason, .. } => return f.write_str(reason),
            // Neither ran long enough to fail for its context.
            LoadCause::NotStarted { reason } => {
                return write!(f, "{server} could not be started for {model}: {reason}");
            }
            LoadCause::PortTaken { port } => {
                return write!(
                    f,
                    "{server} was started for {model} on port {port}, and another program \
                     answers there"
                );
            }
            LoadCause::Stopped { failure, said } => {
                write!(f, "{server} stopped while loading {model}")?;
                match failure {
                    // The loader's words, already read from what it printed.
                    Some(
                        failure @ (Failure::MissingLibrary { .. } | Failure::BadLibrary { .. }),
                    ) => write!(f, ": {failure}")?,
                    Some(failure) => {
                        write!(f, ": {failure}")?;
                        printed(f, said.as_deref())?;
                    }
                    None if said.is_some() => printed(f, said.as_deref())?,
                    None => f.write_str(", before it was ready")?,
                }
            }
            LoadCause::NotReady { waited_secs, said } => {
                write!(
                    f,
                    "{server} was not ready within {waited_secs} s of starting to load {model}"
                )?;
                if said.is_some() {
                    printed(f, said.as_deref())?;
                } else {
                    f.write_str(", and printed nothing")?;
                }
            }
        }
        if let Some(context) = self.fitted_context {
            write!(
                f,
                " — context {context} was fitted to this machine; set a global default to \
                 override it, or GGLIB_DISABLE_CONTEXT_FIT=1 to fall back to {}",
                crate::settings::DEFAULT_CONTEXT_SIZE
            )?;
        }
        Ok(())
    }
}

/// `. It printed: <said>`, when there is a line.
fn printed(f: &mut fmt::Formatter<'_>, said: Option<&str>) -> fmt::Result {
    said.map_or(Ok(()), |said| write!(f, ". It printed: {said}"))
}
