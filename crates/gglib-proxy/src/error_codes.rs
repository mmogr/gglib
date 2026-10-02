//! The error codes gglib writes, as one table, and `docs/error-codes.json`
//! rendered from it (#1119).
//!
//! Clients match on these codes, so the list is published as data rather
//! than left in the Rust. The tests hold three things together: the
//! published file is this table rendered, every code a crate writes is in
//! the table, and every code in the table is written somewhere. What counts
//! as writing a code, and what the scan cannot see, is in `error_codes_scan`.
//!
//! A code relayed from llama-server or from a far machine is not gglib's and
//! is not here. To add a code, write it, add its row here, and run
//! `GGLIB_WRITE_ERROR_CODES=1 cargo test -p gglib-proxy error_codes` to
//! rewrite the file.

use serde::Serialize;

/// The published file's shape. A change that would break a client that reads
/// version 0 is a new version.
const VERSION: u32 = 0;

/// The published file, from the repository root.
const PUBLISHED: &str = "docs/error-codes.json";

const ABOUT: &str = "The error codes gglib writes itself, rather than relays from \
llama-server, on its proxy and the daemon's runs. The proxy refuses with \
{\"error\":{\"message\":...,\"type\":...,\"code\":...}}, and a stream already under way can end \
with that object as a data: frame. A run that failed carries {\"code\":...,\"message\":...} as its \
error, on /v1/runs and /api/runs. The daemon's /api/runs refuses with \
{\"error\":...,\"status\":...,\"type\":<code>}, and the proxy answers a paired device's turn on a \
hub chat with the daemon's refusal of it. The daemon's guards in front of /api refuse with \
upper-case types of their own, such as HOST_NOT_ALLOWED, which are not listed here. Match on \
code. type is the type the proxy writes beside the code: null where it writes more than one, \
or none. status is the HTTP status of every response that carries the code: null where those \
responses' writers do not all fix the same one, or where no response carries it, only a stream \
frame or a run's error.";

/// One code: what a client matches on, the `type` written beside it, the
/// status of the response that carries it, and a line on what it means.
#[derive(Serialize)]
struct ErrorCode {
    code: &'static str,
    #[serde(rename = "type")]
    kind: Kind,
    status: Option<u16>,
    meaning: &'static str,
}

/// The `type` the proxy writes beside a code, published as it or as null.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// This one, wherever the proxy writes the code.
    One(&'static str),
    /// More than one: it depends on the writer, or is llama-server's.
    Many,
    /// None: the proxy never writes the code beside a type.
    Untyped,
}

impl Serialize for Kind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::One(kind) => serializer.serialize_str(kind),
            Self::Many | Self::Untyped => serializer.serialize_none(),
        }
    }
}

const fn row(
    code: &'static str,
    kind: Kind,
    status: Option<u16>,
    meaning: &'static str,
) -> ErrorCode {
    ErrorCode {
        code,
        kind,
        status,
        meaning,
    }
}

const INVALID: Kind = Kind::One("invalid_request_error");
const SERVER: Kind = Kind::One("server_error");
const UNAVAILABLE: Kind = Kind::One("service_unavailable");
const RATE: Kind = Kind::One("rate_limit_error");
const MANY: Kind = Kind::Many;
const UNTYPED: Kind = Kind::Untyped;

/// Every code, one row each: rustfmt would give every field of a row its own
/// line, and the table would stop reading as one.
#[rustfmt::skip]
const CODES: &[ErrorCode] = &[
    // The proxy's own refusals: `ErrorResponse`'s constructors.
    row("model_loading", UNAVAILABLE, Some(503), "The model is still loading; retry shortly."),
    row("admission_timeout", UNAVAILABLE, Some(503), "The request waited its limit for the model without being admitted; retry shortly."),
    row("model_not_found", INVALID, Some(404), "No model by that name is in the catalog."),
    row("profile_not_found", INVALID, Some(404), "No model has that name, and its :suffix names no inference profile."),
    row("not_an_embedding_model", INVALID, Some(400), "An embeddings request named a model not tagged embedding."),
    row("embedding_model_cannot_chat", INVALID, Some(400), "A chat request named an embedding model."),
    row("upstream_error", MANY, None, "The model server (for a run, the proxy) failed, could not be reached, or ended a reply early; the type beside it may be llama-server's."),
    row("context_length_exceeded", Kind::One("context_length_exceeded"), Some(400), "The conversation does not fit the model's context even trimmed; start a new one."),
    row("loop_detected", Kind::One("loop_detected"), Some(400), "The history repeats the same tool calls with the same results, so the request was refused or the agent run stopped."),
    row("stagnation_detected", Kind::One("stagnation_detected"), Some(400), "The assistant gave the same reply too often, so the request was refused or the agent run stopped."),
    row("invalid_request", INVALID, Some(400), "The request could not be read or is not valid."),
    row("internal_error", SERVER, None, "gglib failed on its own side; the message says where."),
    row("model_file_not_found", INVALID, Some(404), "The model is in the catalog but its file is missing."),
    row("pinned_model_mismatch", INVALID, Some(404), "This endpoint serves one pinned model, and the request named another."),
    // The guards in front of every route.
    row("host_not_allowed", INVALID, Some(403), "The Host is neither loopback nor a host allowed with --allowed-host."),
    row("origin_not_allowed", INVALID, Some(403), "A page on another site sent a change."),
    row("invalid_api_key", INVALID, Some(401), "The bearer token is missing or wrong."),
    row("device_not_paired", INVALID, Some(403), "A request came through the tunnel without a paired device's key."),
    row("mcp_not_allowed_over_tunnel", INVALID, Some(403), "This machine does not open its MCP gateway to the tunnel."),
    // The runs and the hub's chats.
    row("device_not_named", INVALID, Some(403), "The route serves a paired device through the tunnel, and the request did not name one."),
    row("runs_unavailable", UNAVAILABLE, Some(503), "This proxy runs outside the gglib daemon, so it holds no runs."),
    row("chats_unavailable", UNAVAILABLE, Some(503), "This proxy runs outside the gglib daemon, so it holds no chats."),
    row("not_found", INVALID, Some(404), "Nothing with that id is visible to the caller."),
    row("conflict", INVALID, Some(409), "The run id is taken, or the conversation already has a live reply."),
    row("not_yours", INVALID, Some(403), "The run belongs to a paired device, so only that device may read its reply."),
    row("too_many_runs", RATE, Some(429), "Too many runs are still going; cancel one or wait."),
    row("shutting_down", MANY, Some(503), "The daemon is stopping, so it starts no more runs."),
    // The daemon's refusals of an agent run, relayed to a device's turn.
    row("conversation_not_found", INVALID, Some(404), "No conversation has that id."),
    row("agent_busy", RATE, Some(429), "Every agent loop slot is in use; retry later."),
    row("no_model", INVALID, Some(422), "The chat names no model and nothing is running on the hub."),
    row("model_unavailable", SERVER, Some(503), "The chat's model could not be loaded on the hub."),
    row("unavailable", SERVER, Some(503), "Something the run needs is unavailable, such as a model stopped while the run was prepared; retry."),
    // /api/runs alone, where the code is the refusal's type.
    row("message_not_found", UNTYPED, Some(404), "The message a run was to replace is not in its conversation."),
    // Inside a stream already under way.
    row("upstream_timeout", SERVER, Some(504), "The model server sent nothing, or did not finish, within its time limit; retry."),
    // A run's error, which no response carries on its own.
    row("run_panicked", UNTYPED, None, "The run stopped on an internal failure."),
    row("proxy_not_running", UNTYPED, None, "The proxy was not running, so the chat run had nowhere to go."),
    row("log_full", UNTYPED, None, "The reply passed the 8 MB a run may hold."),
    row("max_iterations", UNTYPED, None, "The agent reached its iteration limit without a final answer."),
    row("too_many_tool_calls", UNTYPED, None, "The model asked for more tool calls at once than are allowed."),
    row("agent_error", UNTYPED, None, "The agent loop failed; the run's last event says why."),
    row("transcript_not_saved", UNTYPED, None, "The reply could not be saved to its conversation."),
];

/// The published file's whole text.
fn render() -> String {
    #[derive(Serialize)]
    struct Published {
        version: u32,
        about: &'static str,
        codes: &'static [ErrorCode],
    }
    let published = Published {
        version: VERSION,
        about: ABOUT,
        codes: CODES,
    };
    let mut text = serde_json::to_string_pretty(&published).expect("the table serializes");
    text.push('\n');
    text
}

#[path = "error_codes_tokens.rs"]
mod error_codes_tokens;

#[path = "error_codes_items.rs"]
mod error_codes_items;

#[path = "error_codes_scan.rs"]
mod error_codes_scan;

#[path = "error_codes_tests.rs"]
mod error_codes_tests;

#[path = "error_codes_scan_tests.rs"]
mod error_codes_scan_tests;
