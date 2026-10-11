//! Daemon API route constants — the paths the CLI sends to `gglib daemon` —
//! and the keys of the request bodies it sends there.
//!
//! These live here, in shared vocabulary, rather than inside the CLI, so the
//! daemon's own test suite can walk them and fail when it stops serving one.
//! `gglib-axum/tests/daemon_route_contract.rs` is what ties the client's paths
//! to the router's; without it, deleting a route the CLI still calls leaves
//! the whole suite green (#834).
//!
//! The two ends of a request body cannot meet in one test. The CLI's struct is
//! `pub(crate)` inside `gglib-cli`'s `pub(crate) mod daemon_client`, and the
//! daemon's is `pub(crate)` inside `gglib-axum`'s `pub(crate) mod handlers`;
//! both crates deny `unreachable_pub`, and gglib-axum may not depend on
//! gglib-cli. So each side pins itself against a `*_FIELDS` list here instead
//! — the same trick [`CLI_ROUTE_CONTRACT`] uses for paths.

/// Daemon identity probe.
pub const HEALTH_PATH: &str = "/health";

/// Which build of gglib the daemon is running.
///
/// Not in [`CLI_ROUTE_CONTRACT`]: the CLI reads its own compiled-in constants
/// and takes the daemon's from [`HEALTH_PATH`]. This route exists for the
/// dashboard, which has no compiled-in version of its own.
pub const VERSION_PATH: &str = "/api/version";

/// Start the proxy.
pub const PROXY_START_PATH: &str = "/api/proxy/start";

/// Stop the proxy.
pub const PROXY_STOP_PATH: &str = "/api/proxy/stop";

/// Current proxy status.
pub const PROXY_STATUS_PATH: &str = "/api/proxy/status";

/// The loop guard's log, a day per row (#1052).
///
/// Not in [`CLI_ROUTE_CONTRACT`]: `gglib proxy trips` reads the log from this
/// machine's database directly, daemon or not. This route exists for the
/// GUI's settings panel.
pub const PROXY_LOOP_GUARD_TRIPS_PATH: &str = "/api/proxy/loop-guard-trips";

/// The model servers the daemon is running, each with its runtime.
pub const SERVERS_LIST_PATH: &str = "/api/servers";

/// Start (or reuse) a llama-server for a model.
pub const SERVERS_START_PATH: &str = "/api/servers/start";

/// Ask the daemon to shut down.
pub const DAEMON_SHUTDOWN_PATH: &str = "/api/daemon/shutdown";

/// The daemon's event stream. `GET` is the stream the app reads. `POST`
/// puts one event on it: a `gglib` command that changed the library in its
/// own process sends the event its change emitted there.
pub const EVENTS_PATH: &str = "/api/events";

/// Bring the remote tunnel up and arm a pairing (ADR 0012).
pub const REMOTE_ENABLE_PATH: &str = "/api/remote/enable";

/// Take the remote tunnel down.
pub const REMOTE_DISABLE_PATH: &str = "/api/remote/disable";

/// The remote tunnel's status.
pub const REMOTE_STATUS_PATH: &str = "/api/remote/status";

/// Reach another machine's proxy over the tunnel: bind a loopback port here.
pub const REMOTE_JOIN_PATH: &str = "/api/remote/join";

/// Close that loopback port.
pub const REMOTE_DISCONNECT_PATH: &str = "/api/remote/disconnect";

/// Stop the far daemon through the tunnel, then disconnect.
pub const REMOTE_KILL_PATH: &str = "/api/remote/kill";

/// Mint a key for one new device and offer a code that hands it over.
///
/// Needs the tunnel already up; `POST /api/remote/enable` with `invite`
/// still does both in one call, for a first run.
pub const REMOTE_INVITE_PATH: &str = "/api/remote/invite";

/// Every device this machine has issued a key to.
pub const REMOTE_DEVICES_PATH: &str = "/api/remote/devices";

/// The far machine's chats, read through the tunnel for this machine's chat
/// page: `GET` lists them. Not called by the CLI.
pub const REMOTE_CHATS_PATH: &str = "/api/remote/chats";

/// One far chat, interpolating `id` into [`REMOTE_CHATS_PATH`]: `GET` opens it.
#[must_use]
pub fn remote_chat_path(id: i64) -> String {
    format!("{REMOTE_CHATS_PATH}/{id}")
}

/// The verbs [`remote_chat_path`] is called with.
pub const REMOTE_CHAT_METHODS: &[&str] = &["GET"];

/// A turn on a far chat under run `run_id`: `PUT` adds it. The caller owes
/// the run id's charset, as for [`run_path`].
#[must_use]
pub fn remote_turn_path(id: i64, run_id: &str) -> String {
    format!("{REMOTE_CHATS_PATH}/{id}/turns/{run_id}")
}

/// The verbs [`remote_turn_path`] is called with.
pub const REMOTE_TURN_METHODS: &[&str] = &["PUT"];

/// The far machine's runs this device may see: `GET` lists them.
pub const REMOTE_RUNS_PATH: &str = "/api/remote/runs";

/// A far run's events after `after`, as server-sent events.
#[must_use]
pub fn remote_run_events_path(id: &str, after: u32) -> String {
    format!("{REMOTE_RUNS_PATH}/{id}/events?after={after}")
}

/// Cancel a far run.
#[must_use]
pub fn remote_run_cancel_path(id: &str) -> String {
    format!("{REMOTE_RUNS_PATH}/{id}/cancel")
}

/// The paired machine's models, read through the tunnel: `GET` lists every
/// entry its `/v1/models` publishes, profile variants included.
pub const REMOTE_MODELS_PATH: &str = "/api/remote/models";

/// One of the paired machine's models, by an identifier that machine resolves
/// (an id or a name, either with `:<profile>`), encoded as one segment by
/// [`super::path_segment`]: `GET` reads it.
#[must_use]
pub fn remote_model_path(model: &str) -> String {
    format!("{REMOTE_MODELS_PATH}/{}", super::path_segment(model))
}

/// Have one of the paired machine's models resident now: `POST`.
#[must_use]
pub fn remote_model_load_path(model: &str) -> String {
    format!("{}/load", remote_model_path(model))
}

/// The routes to the far machine, each with the verbs the chat page or the
/// CLI sends, the parameterized ones instantiated. Beside
/// [`CLI_ROUTE_CONTRACT`], for the same sweep.
#[must_use]
pub fn remote_route_contract() -> Vec<(&'static [&'static str], String)> {
    let mut routes: Vec<(&'static [&'static str], String)> = vec![
        (&["GET"], REMOTE_CHATS_PATH.to_owned()),
        (REMOTE_CHAT_METHODS, remote_chat_path(12)),
        (REMOTE_TURN_METHODS, remote_turn_path(12, "chat-1")),
        (&["GET"], REMOTE_RUNS_PATH.to_owned()),
        (RUN_EVENTS_METHODS, remote_run_events_path("chat-1", 0)),
        (RUN_CANCEL_METHODS, remote_run_cancel_path("chat-1")),
        (&["GET"], REMOTE_MODELS_PATH.to_owned()),
        (&["GET"], remote_model_path("org/qwen3:coding")),
        (&["POST"], remote_model_load_path("org/qwen3:coding")),
    ];
    routes.extend(super::attachments::remote_route_contract());
    routes
}

/// Download queue: `POST` enqueues, `GET` returns the snapshot.
///
/// One path for both verbs. The snapshot handler was once double-mounted at
/// `/api/models/downloads` as well; when that mount was retired the CLI was
/// still polling it, and the bare path fell through to `/api/models/{id}`,
/// whose `i64` extractor answers `400 text/plain`.
pub const DOWNLOADS_QUEUE_PATH: &str = "/api/models/downloads/queue";

/// Model list: the library, sorted and filtered as its query parameters ask.
pub const MODELS_LIST_PATH: &str = "/api/models";

/// Repair one model, interpolating `id` into [`MODELS_LIST_PATH`]: delete its
/// unhealthy files and queue the download that fetches them again.
#[must_use]
pub fn model_repair_path(id: i64) -> String {
    format!("{MODELS_LIST_PATH}/{id}/repair")
}

/// The verbs [`model_repair_path`] is called with.
pub const MODEL_REPAIR_METHODS: &[&str] = &["POST"];

/// Benchmark comparison run (SSE).
pub const BENCHMARK_COMPARE_PATH: &str = "/api/benchmark/compare";

/// Benchmark performance run (SSE).
pub const BENCHMARK_PERF_PATH: &str = "/api/benchmark/perf";

/// Benchmark tuning run (SSE).
pub const BENCHMARK_TUNE_PATH: &str = "/api/benchmark/tune";

/// Agentic evaluation run (SSE).
pub const BENCHMARK_AGENTIC_PATH: &str = "/api/benchmark/agentic";

/// Setup status, used for the hardware snapshot on benchmark reports.
pub const SETUP_STATUS_PATH: &str = "/api/config/system/setup-status";

/// Apply a gated tune run, interpolating `run_id` into [`BENCHMARK_TUNE_PATH`].
#[must_use]
pub fn benchmark_tune_apply_path(run_id: i64) -> String {
    format!("{BENCHMARK_TUNE_PATH}/{run_id}/apply")
}

/// Runs, a reply the daemon owns until it ends: `GET` lists them.
pub const RUNS_PATH: &str = "/api/runs";

/// One run, interpolating `id` into [`RUNS_PATH`].
///
/// `PUT` starts it and `GET` reads it. The caller owes the charset, as for
/// [`remote_forget_path`]: `gglib run` checks an id with
/// `domain::runs::is_run_id` first.
#[must_use]
pub fn run_path(id: &str) -> String {
    format!("{RUNS_PATH}/{id}")
}

/// The verbs [`run_path`] is called with.
pub const RUN_METHODS: &[&str] = &["GET", "PUT"];

/// A run's events after `after`, as server-sent events.
#[must_use]
pub fn run_events_path(id: &str, after: u32) -> String {
    format!("{RUNS_PATH}/{id}/events?after={after}")
}

/// The verbs [`run_events_path`] is called with.
pub const RUN_EVENTS_METHODS: &[&str] = &["GET"];

/// Cancel a run.
#[must_use]
pub fn run_cancel_path(id: &str) -> String {
    format!("{RUNS_PATH}/{id}/cancel")
}

/// The verbs [`run_cancel_path`] is called with.
pub const RUN_CANCEL_METHODS: &[&str] = &["POST"];

/// Every fixed path above, paired with the verbs the CLI sends to it.
///
/// The verb is half the contract: a deleted route often still *matches* some
/// parameterized sibling, and only the method it allows gives that away.
pub const CLI_ROUTE_CONTRACT: &[(&[&str], &str)] = &[
    (&["GET"], HEALTH_PATH),
    (&["POST"], PROXY_START_PATH),
    (&["POST"], PROXY_STOP_PATH),
    (&["GET"], PROXY_STATUS_PATH),
    (&["GET"], SERVERS_LIST_PATH),
    (&["POST"], SERVERS_START_PATH),
    (&["POST"], DAEMON_SHUTDOWN_PATH),
    (&["POST"], EVENTS_PATH),
    (&["POST"], REMOTE_ENABLE_PATH),
    (&["POST"], REMOTE_DISABLE_PATH),
    (&["GET"], REMOTE_STATUS_PATH),
    (&["POST"], REMOTE_JOIN_PATH),
    (&["POST"], REMOTE_DISCONNECT_PATH),
    (&["POST"], REMOTE_KILL_PATH),
    (&["POST"], REMOTE_INVITE_PATH),
    (&["GET"], REMOTE_DEVICES_PATH),
    (&["GET"], REMOTE_MODELS_PATH),
    (&["GET", "POST"], DOWNLOADS_QUEUE_PATH),
    (&["GET"], MODELS_LIST_PATH),
    (&["POST"], BENCHMARK_COMPARE_PATH),
    (&["POST"], BENCHMARK_PERF_PATH),
    (&["POST"], BENCHMARK_TUNE_PATH),
    (&["POST"], BENCHMARK_AGENTIC_PATH),
    (&["GET"], SETUP_STATUS_PATH),
    (&["GET"], RUNS_PATH),
];

/// The `type` of the daemon's 401 when it wanted its token, so a client can
/// tell it from the API key's `INVALID_API_KEY` and show
/// [`DAEMON_TOKEN_REQUIRED_MESSAGE`] rather than ask for a key.
pub const DAEMON_TOKEN_REQUIRED_TYPE: &str = "DAEMON_TOKEN_REQUIRED";

/// The `error` of that 401: what it wants and how to get it.
pub const DAEMON_TOKEN_REQUIRED_MESSAGE: &str = "this route needs the daemon's token: run the \
     command from `gglib` on this machine, or open the page from the link `gglib web` prints";

/// The verbs [`benchmark_tune_apply_path`] is called with.
pub const BENCHMARK_TUNE_APPLY_METHODS: &[&str] = &["POST"];

/// Retire one device, interpolating `device` into [`REMOTE_DEVICES_PATH`].
///
/// The id is a path segment rather than a body, because `DELETE` with one is
/// poorly served by enough of the stack to be worth avoiding, and because
/// the daemon already names a resource this way at `/api/mcp/servers/{id}`.
/// **Not escaped, and the caller owes the charset.** Every id this crate
/// mints is `[A-Za-z0-9._-]` and the edge holds tokens under nothing else,
/// so an id that came from the roster is safe to interpolate. One that came
/// from a person's shell is not: an HTTP client resolves dot-segments the
/// way a browser does, so `../../models/7` here is a `DELETE` of a different
/// route. `gglib remote forget` checks the shape before it calls this; any
/// new caller taking an id from outside the roster must do the same.
#[must_use]
pub fn remote_forget_path(device: &str) -> String {
    format!("{REMOTE_DEVICES_PATH}/{device}")
}

/// The verbs [`remote_forget_path`] is called with.
pub const REMOTE_FORGET_METHODS: &[&str] = &["DELETE"];

/// Every key the CLI puts in a `POST /api/proxy/start` body: `StartProxyBody`
/// there, `StartProxyConfig` in the daemon.
pub const PROXY_START_CLI_FIELDS: &[&str] = &[
    "host",
    "port",
    "default_context",
    "cache",
    "slot_dir",
    "pinned",
    "cache_disk_gb",
    "inference_override",
    "default_profile",
    "api_key",
    "allowed_hosts",
];

/// Keys the daemon accepts on that body which the CLI never sends.
///
/// `llama_base_port` is read only by `POST /api/proxy/start-pinned`, which
/// routes it through the launch cascade. `/api/proxy/start` deserializes it and
/// never looks at it, so it is daemon-only by function rather than by omission.
pub const PROXY_START_DAEMON_ONLY_FIELDS: &[&str] = &["llama_base_port"];

/// Every key the CLI puts in a `POST /api/servers/start` body: `StartServerBody`
/// at both ends, each an id beside a flattened `StartServerRequest`.
///
/// The daemon's body refuses no unknown key, so a key spelled another way is
/// dropped without an error, and the setting it carried with it.
pub const SERVERS_START_CLI_FIELDS: &[&str] = &[
    "id",
    "contextLength",
    "port",
    "jinja",
    "reasoningFormat",
    "mtpDraftNMax",
    "mtpDraftPMin",
    "inferenceParams",
    "mlock",
];
