//! The keys `gglib config set` and `unset` take, and what each one takes.
//!
//! One row per key, named as `gglib config show` prints it. The table is the
//! list `config set --help` shows, the list clap refuses an unknown key
//! against, and the list `scripts/check_settings_surfaces.sh` reads to prove
//! no `Settings` field is stranded without a way to set it. Its tests fail
//! for a field the wire type or `show` has and the table does not.

use clap::ValueEnum;
use clap::builder::PossibleValue;
use gglib_core::domain::agent::MIN_TOOL_TIMEOUT_MS;
use gglib_core::settings::{CONTEXT_SIZE_RANGE, DOWNLOAD_QUEUE_RANGE, MIN_CACHE_DISK_GB, MIN_PORT};
use gglib_core::{MAX_ITERATIONS_CEILING, MAX_PARALLEL_TOOLS_CEILING, MAX_TOOL_TIMEOUT_MS_CEILING};

/// The range a number is held to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Bound {
    /// Refused outside `lo-hi` when stored.
    Closed(u64, u64),
    /// Refused below the value when stored.
    AtLeast(u64),
    /// Stored as given, and clamped to `lo-hi` where an agent loop reads it.
    Clamped(u64, u64),
    Unbounded,
}

/// What a key takes as its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Takes {
    Number(Bound),
    /// `true` or `false`.
    Switch,
    Text,
    /// A comma-separated list of hosts.
    Hosts,
    /// One of these words.
    Choice(&'static [&'static str]),
    /// A model in the library, by id or name; its id is stored.
    Model,
    /// A folder, created when missing.
    Folder,
    /// A value too large for one argument: `set` refuses it and says where
    /// it is edited, and `unset` clears it.
    Whole(&'static str),
    /// A value another command writes: `set` and `unset` both refuse it and
    /// say which.
    Elsewhere(&'static str),
}

/// One key of `gglib config set`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettingKey {
    /// The key, as `config show` prints it.
    pub(crate) name: &'static str,
    pub(crate) takes: Takes,
    /// What it is, for `--help`.
    about: &'static str,
    /// Left out of `--help` and of clap's list of keys, and still accepted.
    hidden: bool,
}

const fn key(name: &'static str, takes: Takes, about: &'static str) -> SettingKey {
    let hidden = matches!(takes, Takes::Elsewhere(_));
    SettingKey {
        name,
        takes,
        about,
        hidden,
    }
}

/// Every key, in the order `config show` prints them. `models-dir` is a
/// line in the data root's `.env` rather than a setting.
pub(crate) const KEYS: &[SettingKey] = &[
    key(
        "agentic-sampling",
        Takes::Switch,
        "Cap the temperature nobody chose on turns that carry tools (on when unset)",
    ),
    key(
        "close-to-tray",
        Takes::Switch,
        "Closing the desktop app's window hides it to the tray, leaving the endpoint serving",
    ),
    key(
        "default-context-size",
        Takes::Number(Bound::Closed(
            *CONTEXT_SIZE_RANGE.start(),
            *CONTEXT_SIZE_RANGE.end(),
        )),
        "Context size every launch uses; unset, each launch is sized to this machine",
    ),
    key(
        "default-image-model-id",
        Takes::Model,
        "The model a drawing request uses when it names none; refused unless it draws",
    ),
    key(
        "default-model-id",
        Takes::Model,
        "The model a command uses when none is named, as `gglib q` without -m",
    ),
    key(
        "inference-defaults",
        Takes::Whole("edited under Settings › Models in the app"),
        "Sampling defaults for every model",
    ),
    key(
        "inference-profiles",
        Takes::Whole("edited with `gglib config profile`"),
        "Named sampling profiles, selected as <model>:<profile>",
    ),
    key(
        "llama-base-port",
        Takes::Number(Bound::AtLeast(MIN_PORT as u64)),
        "First port llama-server instances are given, behind the endpoint",
    ),
    key(
        "loop-guard-mode",
        Takes::Choice(&["off", "note", "refuse"]),
        "What the endpoint does with a conversation that repeats itself (note when unset)",
    ),
    key(
        "max-download-queue-size",
        Takes::Number(Bound::Closed(
            *DOWNLOAD_QUEUE_RANGE.start() as u64,
            *DOWNLOAD_QUEUE_RANGE.end() as u64,
        )),
        "Downloads that can wait in the queue",
    ),
    key(
        "max-parallel-tools",
        Takes::Number(Bound::Clamped(1, MAX_PARALLEL_TOOLS_CEILING as u64)),
        "Tool calls an agent run makes at once, where its chat names none",
    ),
    key(
        "max-stagnation-steps",
        Takes::Number(Bound::Unbounded),
        "Steps without progress before an agent run stops",
    ),
    key(
        "max-tool-iterations",
        Takes::Number(Bound::Clamped(1, MAX_ITERATIONS_CEILING as u64)),
        "Steps an agent run takes at most",
    ),
    key(
        "mcp-drawing",
        Takes::Switch,
        "Offer MCP clients gglib's drawing tool on /mcp (off when unset)",
    ),
    key(
        "models-dir",
        Takes::Folder,
        "Where downloads go; `GGLIB_MODELS_DIR` in the environment outranks it",
    ),
    key(
        "proxy-allowed-hosts",
        Takes::Hosts,
        "Host names the endpoint answers to beyond loopback and its own address",
    ),
    key(
        "proxy-api-key",
        Takes::Text,
        "Bearer token the endpoint demands on /v1/* and /mcp; unset, it demands none",
    ),
    key(
        "proxy-autostart",
        Takes::Switch,
        "Start the endpoint as soon as the desktop app launches",
    ),
    key(
        "proxy-cache-disk-gb",
        Takes::Number(Bound::AtLeast(MIN_CACHE_DISK_GB)),
        "Budget in GiB for the on-disk prompt cache; outranks GGLIB_CACHE_DISK_GB",
    ),
    key(
        "proxy-cache-on-disk",
        Takes::Switch,
        "Keep the endpoint's prompt cache on disk across model swaps and restarts",
    ),
    key(
        "proxy-host",
        Takes::Text,
        "Address the endpoint binds, an IP such as 127.0.0.1 or 0.0.0.0; off loopback it demands the API key",
    ),
    key(
        "proxy-port",
        Takes::Number(Bound::AtLeast(MIN_PORT as u64)),
        "Port the endpoint listens on",
    ),
    key(
        "remote-devices",
        Takes::Elsewhere("`gglib remote invite` and `gglib remote forget` keep it"),
        "The devices this machine admits",
    ),
    key(
        "remote-enabled",
        Takes::Elsewhere("`gglib remote enable` and `gglib remote disable` switch it"),
        "Whether remote access is on",
    ),
    key(
        "remote-pairing",
        Takes::Elsewhere("`gglib remote join` writes it"),
        "The machine this one joined",
    ),
    key(
        "remote-serve",
        Takes::Elsewhere("`gglib remote enable` records it"),
        "How remote access is served",
    ),
    SettingKey {
        hidden: true,
        ..key(
            "setup-completed",
            Takes::Switch,
            "Whether first-run setup has finished",
        )
    },
    key(
        "start-at-login",
        Takes::Switch,
        "Launch the desktop app at login",
    ),
    key(
        "title-generation-prompt",
        Takes::Text,
        "The prompt a chat's title is written from",
    ),
    key(
        "tool-call-repair",
        Takes::Switch,
        "Re-issue a malformed tool call under a grammar (on when unset)",
    ),
    key(
        "tool-timeout-ms",
        Takes::Number(Bound::Clamped(
            MIN_TOOL_TIMEOUT_MS,
            MAX_TOOL_TIMEOUT_MS_CEILING,
        )),
        "How long one tool call may take, in milliseconds, where its chat names none",
    ),
    key(
        "trust-client-sampling",
        Takes::Switch,
        "Honour a client's own sampling parameters (off when unset)",
    ),
];

impl Takes {
    /// What the value is, as `--help` and a refusal say it.
    pub(crate) fn described(self) -> String {
        match self {
            Self::Number(bound) => match bound {
                Bound::Closed(lo, hi) => format!("a whole number ({lo}-{hi})"),
                Bound::AtLeast(lo) => format!("a whole number (>= {lo})"),
                Bound::Clamped(lo, hi) => {
                    format!("a whole number (clamped to {lo}-{hi} when a loop runs)")
                }
                Bound::Unbounded => "a whole number".to_owned(),
            },
            Self::Switch => "true or false".to_owned(),
            Self::Text => "text".to_owned(),
            Self::Hosts => "hosts, comma-separated, each with or without a port".to_owned(),
            Self::Choice(words) => match words {
                [] => String::new(),
                [only] => (*only).to_owned(),
                [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
            },
            Self::Model => "a model's id or name".to_owned(),
            Self::Folder => "a path, created when missing".to_owned(),
            Self::Whole(where_) => format!("not one value; {where_}"),
            Self::Elsewhere(who) => format!("not set here; {who}"),
        }
    }
}

impl SettingKey {
    /// The line `--help` shows for the key.
    pub(crate) fn help(&self) -> String {
        format!("{}: {}", self.about, self.takes.described())
    }
}

impl ValueEnum for SettingKey {
    fn value_variants<'a>() -> &'a [Self] {
        KEYS
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        Some(
            PossibleValue::new(self.name)
                .help(self.help())
                .hide(self.hidden),
        )
    }
}

#[cfg(test)]
#[path = "setting_key_tests.rs"]
mod tests;
