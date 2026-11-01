//! Every route the tunnel carries is reached by a command, or exempted with
//! the reason none does.
//!
//! `gglib-proxy`'s `router_tests.rs` holds [`TUNNEL_REACHABLE`] to the routes
//! the proxy registers; this holds the same list to the commands that use
//! them. Nothing links a route to a command mechanically, so the table is
//! written by hand and pinned at both ends: a route the paired machine opens
//! cannot go unnoticed here, and a command [`reach`] lets use the paired
//! machine has to name a route it uses.

use std::collections::BTreeSet;

use clap::Parser as _;
use gglib_proxy::tunnel_routes::TUNNEL_REACHABLE;

use super::{REACHES, Reach, reach};
use crate::parser::Cli;

/// What reaches a route: the command lines that do, or why none does.
enum Reached {
    By(&'static [&'static [&'static str]]),
    Exempt(&'static str),
}

use Reached::{By, Exempt};

/// Each route the tunnel carries, once. The command lines are the ones
/// `--remote` sends there, directly or through this machine's daemon.
const ROUTES: &[(&str, Reached)] = &[
    (
        "/health",
        Exempt("a probe for a supervisor; it reports no state a command shows"),
    ),
    ("/v1/models", By(&[&["gglib", "model", "list"]])),
    ("/v1/models/{name}/load", By(&[&["gglib", "serve", "7"]])),
    (
        "/v1/models/{name}/detail",
        By(&[
            &["gglib", "model", "inspect", "7"],
            &["gglib", "chat", "7"],
            &["gglib", "q", "hi"],
        ]),
    ),
    (
        "/v1/chat/completions",
        By(&[&["gglib", "chat", "7"], &["gglib", "q", "hi"]]),
    ),
    ("/v1/embeddings", Exempt("no gglib command embeds")),
    (
        "/v1/images/generations",
        Exempt("`gglib image` does not reach it yet (#1707)"),
    ),
    (
        "/v1/images/drawing",
        Exempt(
            "the page asks it for a far chat's Draw button; a run on a far model does not \
             draw (ADR 0018, decision 11)",
        ),
    ),
    (
        "/v1/proxy/status",
        Exempt("`proxy dashboard` reads the stream form; the one-shot read is for scripts"),
    ),
    (
        "/v1/proxy/status/stream",
        By(&[&["gglib", "proxy", "dashboard"]]),
    ),
    (
        "/v1/proxy/cache/clear",
        By(&[&["gglib", "proxy", "cache-clear"]]),
    ),
    ("/v1/proxy/shutdown", By(&[&["gglib", "daemon", "stop"]])),
    (
        "/v1/runs",
        Exempt("a paired device's own runs, which the page lists; `gglib run` is this machine's"),
    ),
    ("/v1/runs/{id}", Exempt("as /v1/runs")),
    ("/v1/runs/{id}/events", Exempt("as /v1/runs")),
    ("/v1/runs/{id}/cancel", Exempt("as /v1/runs")),
    (
        "/v1/chats",
        Exempt("the page's far chats, through the daemon's /api/remote routes"),
    ),
    ("/v1/chats/{id}", Exempt("as /v1/chats")),
    ("/v1/chats/{id}/changes", Exempt("as /v1/chats")),
    ("/v1/chats/{id}/pictures", Exempt("as /v1/chats")),
    ("/v1/attachments", Exempt("as /v1/chats")),
    ("/v1/attachments/{id}", Exempt("as /v1/chats")),
    (
        "/mcp",
        Exempt("the MCP gateway, guarded for the tunnel on its own (ADR 0024)"),
    ),
];

#[test]
fn the_table_names_every_route_the_tunnel_carries_once() {
    for (route, reached) in ROUTES {
        if let Exempt(why) = reached {
            assert!(!why.trim().is_empty(), "{route} is exempt with no reason");
        }
    }
    let named: Vec<&str> = ROUTES.iter().map(|(route, _)| *route).collect();
    let unique: BTreeSet<&str> = named.iter().copied().collect();
    assert_eq!(named.len(), unique.len(), "a route is named twice");
    let carried: BTreeSet<&str> = TUNNEL_REACHABLE.iter().copied().collect();
    assert_eq!(
        unique, carried,
        "the tunnel carries a route this table does not name, or the other way round: \
         say which command reaches it, or why none does"
    );
}

#[test]
fn every_command_the_table_names_uses_the_paired_machine() {
    for (route, reached) in ROUTES {
        let By(lines) = reached else { continue };
        for argv in *lines {
            let command = Cli::parse_from(*argv).command.expect("a subcommand");
            let (name, how) = reach(&command);
            assert_eq!(
                how,
                Reach::Use,
                "{route} names `{name}`, which --remote refuses"
            );
        }
    }
}

#[test]
fn every_command_that_uses_the_paired_machine_reaches_a_route() {
    let reaching: BTreeSet<&str> = ROUTES
        .iter()
        .filter_map(|(_, reached)| match reached {
            By(lines) => Some(*lines),
            Exempt(_) => None,
        })
        .flatten()
        .map(|argv| reach(&Cli::parse_from(*argv).command.expect("a subcommand")).0)
        .collect();
    for name in REACHES.split(", ") {
        assert!(
            reaching.contains(name),
            "--remote reaches `{name}`, but the table names no route it uses"
        );
    }
}
