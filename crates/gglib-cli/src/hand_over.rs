//! What the CLI does about a library a newer gglib upgraded, and what it
//! says after upgrading one itself.
//!
//! Every gglib on a machine shares one library, and an upgrade of it is one
//! way: an older copy refuses the file from then on. A developer with a
//! packaged install and a build from a checkout meets this when `gglib` on
//! `PATH` is the older of the two.
//!
//! So a copy that is refused its library looks at the other copies on
//! `PATH` ([`copies`]) and asks each which library it opens
//! ([`copy_generation`]). If one opens this library, and the copy that was
//! run is itself on `PATH`, the command line is handed to that one and a
//! line on stderr says so. Of several, the one that opens the oldest
//! library that is new enough is chosen, so the hand-over upgrades nothing
//! further where it need not.
//!
//! Nothing is handed over:
//!
//! - from a binary that is not on `PATH`, which is a build run from its own
//!   folder: its developer asked for that build, and is told how to give it
//!   a library of its own;
//! - from `daemon run`, which the CLI and the desktop app start and nobody
//!   types;
//! - by a copy that was itself handed the command ([`HANDED_OVER`]), so a
//!   wrapper named `gglib` cannot make a loop;
//! - on Windows, where a program started through
//!   `gglib_core::utils::process` gets a console of its own.
//!
//! Each of those is refused in a sentence that names the copy to run.
//!
//! A copy that upgrades the library names every other copy on `PATH` that
//! no longer opens it ([`say_who_is_left_behind`]).

use std::path::{Path, PathBuf};

use gglib_db::{NewerLibrary, SCHEMA_GENERATION};

use crate::commands::Commands;
use crate::handlers::config::{copies, copy_generation, copy_version};
use crate::subcommands::DaemonCommand;

/// Set for a copy that is handed a command, which then never hands it on.
pub const HANDED_OVER: &str = "GGLIB_HANDED_OVER";

/// Another `gglib` on `PATH`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Other {
    /// Where it is on `PATH`.
    pub(crate) path: PathBuf,
    /// Its version, if it gave one.
    pub(crate) version: Option<String>,
    /// The newest library generation it opens, if that could be learned.
    pub(crate) opens: Option<i64>,
}

impl Other {
    /// `<path> (<version>)`.
    fn named(&self) -> String {
        let version = self.version.as_deref().unwrap_or("version unknown");
        format!("{} ({version})", self.path.display())
    }
}

/// The copies of gglib on this machine's `PATH`, seen from the running one.
pub(crate) struct Machine {
    /// The running binary, as it is shown.
    pub(crate) running: String,
    /// Whether the running binary is one of the copies on `PATH`.
    pub(crate) on_path: bool,
    /// Every other copy on `PATH`, in its order.
    pub(crate) others: Vec<Other>,
}

impl Machine {
    /// This machine, with each other copy asked its version and its
    /// generation.
    fn here() -> Self {
        Self::of(
            &copies::Search::here(),
            &|bin| copy_version::read_version(bin, copy_version::VERSION_WAIT),
            &|bin, version| {
                copy_generation::read_generation(bin, version, copy_generation::GENERATION_WAIT)
            },
        )
    }

    /// What `search` finds, with `version_of` and `generation_of` asked of
    /// each other copy. A copy with no version is not asked its generation.
    pub(crate) fn of(
        search: &copies::Search,
        version_of: &dyn Fn(&Path) -> Option<String>,
        generation_of: &dyn Fn(&Path, &str) -> Option<i64>,
    ) -> Self {
        let (on_path, paths) = copies::others_on_path(search);
        let others = paths
            .into_iter()
            .map(|path| {
                let version = version_of(&path);
                let opens = version.as_deref().and_then(|v| generation_of(&path, v));
                Other {
                    path,
                    version,
                    opens,
                }
            })
            .collect();
        let running = search.running.as_ref().map_or_else(
            || "gglib".to_owned(),
            |running| running.display().to_string(),
        );
        Self {
            running,
            on_path,
            others,
        }
    }

    /// The copy to run for a library of generation `found`: one that opens
    /// it, and of several the one that opens the oldest library, then the
    /// first on `PATH`.
    pub(crate) fn opener(&self, found: i64) -> Option<&Other> {
        self.others
            .iter()
            .filter(|other| other.opens.is_some_and(|opens| opens >= found))
            .min_by_key(|other| other.opens)
    }
}

/// What `main` returns for `error`, which opening the library gave, with
/// `command` the command that needed it.
///
/// Any error but [`NewerLibrary`] is returned as it is. For that one, the
/// command line is handed to a copy on `PATH` that opens the library, and
/// this does not return; where it is not handed over (the module says
/// when), the error says which copy to run, or that there is none.
#[must_use]
pub fn refused(error: anyhow::Error, command: &Commands) -> anyhow::Error {
    let Some(newer) = error.downcast_ref::<NewerLibrary>() else {
        return error;
    };
    let machine = Machine::here();
    let opener = machine.opener(newer.found);
    if let Some(other) = opener
        && machine.on_path
        && hands_over(command)
    {
        eprintln!("{}", handed(&machine, other));
        let failed = run_instead(&other.path);
        eprintln!("{} could not be run: {failed}", other.path.display());
    }
    anyhow::anyhow!(refusal(newer, &machine, opener))
}

/// Whether this process may hand `command` to another copy at all.
fn hands_over(command: &Commands) -> bool {
    let service = matches!(
        command,
        Commands::Daemon {
            command: DaemonCommand::Run { .. }
        }
    );
    cfg!(unix) && !service && std::env::var_os(HANDED_OVER).is_none()
}

/// The line said before `other` takes the command over.
fn handed(machine: &Machine, other: &Other) -> String {
    format!(
        "gglib {} at {} is too old for your library, so {} is running this command. \
         Update or remove the old one.",
        gglib_build_info::SEMVER,
        machine.running,
        other.named()
    )
}

/// The refusal of a library `newer` describes, on `machine`, where `opener`
/// is the copy that opens it, if there is one.
pub(crate) fn refusal(newer: &NewerLibrary, machine: &Machine, opener: Option<&Other>) -> String {
    let mut said = format!(
        "this gglib ({} at {}) is too old for your library, which a newer gglib upgraded \
         (schema generation {}; this build knows {SCHEMA_GENERATION}).",
        gglib_build_info::SEMVER,
        machine.running,
        newer.found
    );
    said.push_str(&opener.map_or_else(
        || none_opens(&machine.others),
        |other| {
            format!(
                " {} opens it: run that one, and update or remove this one.",
                other.named()
            )
        },
    ));
    if !machine.on_path {
        said.push_str(
            " To keep a development build's library apart from your installed one, set \
             GGLIB_DATA_DIR and GGLIB_DAEMON_PORT for it.",
        );
    }
    // Going back to the backup loses what changed since, so it is offered
    // only where every copy here is known not to open the library as it is.
    if opener.is_none() && machine.others.iter().all(|other| other.opens.is_some()) {
        said.push_str(newer.restore.as_deref().unwrap_or_default());
    }
    said
}

/// The sentence for a library no copy in `others` is known to open, which
/// names each that did not say what it opens: it may open it.
fn none_opens(others: &[Other]) -> String {
    let silent: Vec<String> = others
        .iter()
        .filter(|other| other.opens.is_none())
        .map(Other::named)
        .collect();
    if silent.is_empty() {
        " No other gglib on PATH opens it. Update this one.".to_owned()
    } else {
        format!(
            " No other gglib on PATH is known to open it; {} did not say what it opens. \
             Update this one.",
            silent.join(", ")
        )
    }
}

/// Run `bin` with this process's arguments in its place. It returns only
/// when `bin` could not be started, with why.
#[cfg(unix)]
fn run_instead(bin: &Path) -> std::io::Error {
    use std::os::unix::process::CommandExt as _;
    gglib_core::utils::process::cmd(bin)
        .args(std::env::args_os().skip(1))
        .env(HANDED_OVER, "1")
        .exec()
}

/// Nothing is handed over on Windows ([`hands_over`]).
#[cfg(not(unix))]
fn run_instead(_bin: &Path) -> std::io::Error {
    std::io::Error::from(std::io::ErrorKind::Unsupported)
}

/// After this copy upgraded the library: say on stderr which other copies
/// on `PATH` no longer open it. Nothing is said when every copy does.
pub(crate) fn say_who_is_left_behind() {
    for line in left_behind(&Machine::here().others) {
        eprintln!("{line}");
    }
}

/// The lines [`say_who_is_left_behind`] prints for `others`. A copy whose
/// generation could not be learned is not named.
pub(crate) fn left_behind(others: &[Other]) -> Vec<String> {
    let left: Vec<String> = others
        .iter()
        .filter(|other| other.opens.is_some_and(|opens| opens < SCHEMA_GENERATION))
        .map(|other| format!("  {}", other.named()))
        .collect();
    if left.is_empty() {
        return left;
    }
    let mut lines = vec![format!(
        "Your library was upgraded for gglib {}, and an older gglib here no longer opens it:",
        gglib_build_info::SEMVER
    )];
    lines.extend(left);
    lines.push("Update or remove it.".to_owned());
    lines
}

#[cfg(test)]
#[path = "hand_over_tests.rs"]
mod tests;
