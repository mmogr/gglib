//! What a paired machine reaches through the tunnel, as a list.
//!
//! The remote tunnel (ADR 0012) forwards to this proxy alone, so the routes
//! below are everything a paired machine can ask for. `router_tests.rs` holds
//! the list to the routes `router.rs` registers, and `gglib-cli`'s tests hold
//! each route to the command that reaches it. Built for tests only: nothing
//! at run time reads it.

/// Every path the proxy serves, and therefore every path the tunnel carries.
///
/// `/mcp`, `/v1/chats` and `/v1/attachments` are here with guards
/// of their own. An image a device stores is part of a turn it adds to a
/// chat, which is use: it changes no model and no setting. So is a change
/// to a chat (ADR 0017): it writes the chats a turn writes, adding a branch
/// or replacing a question nothing answers yet, and never a saved reply.
/// So is keeping a picture (ADR 0018, decision 21): it adds a stored image
/// to a tool row of a chat, and alters no saved reply.
pub const TUNNEL_REACHABLE: &[&str] = &[
    "/health",
    "/v1/models",
    "/v1/models/{name}/load",
    "/v1/models/{name}/detail",
    "/v1/chat/completions",
    "/v1/embeddings",
    "/v1/images/generations",
    "/v1/images/drawing",
    "/v1/proxy/status",
    "/v1/proxy/status/stream",
    "/v1/proxy/cache/clear",
    "/v1/proxy/shutdown",
    "/v1/runs",
    "/v1/runs/{id}",
    "/v1/runs/{id}/events",
    "/v1/runs/{id}/cancel",
    "/v1/chats",
    "/v1/chats/{id}",
    "/v1/chats/{id}/changes",
    "/v1/chats/{id}/pictures",
    "/v1/attachments",
    "/v1/attachments/{id}",
    "/mcp",
];
