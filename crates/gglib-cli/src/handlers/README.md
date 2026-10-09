# handlers

<!-- module-docs:start -->

CLI command handlers implementing the business logic for each command.

## Purpose

This module contains the **handler functions** that implement the actual logic for CLI commands. `dispatch.rs` calls them with the arguments clap has parsed.

## Architecture Pattern

**Separation of Concerns**

```text
┌─────────────────────────────────────────────────────────────┐
│                      CLI Flow                               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  User Input → Parser → Handler → Service → Port → Adapter  │
│     (clap)   (parser.rs) (this)  (core)   (core)  (infra)  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**Handlers** sit between the CLI parser and the service layer:
- Extract validated arguments from parser
- Format inputs for service calls
- Handle errors and format output
- Present results to user

## Handler Organization

Handlers are grouped by the command they serve, one directory per family. The
list below is the directory tree, not a summary of it — an earlier version of
this section described `add.rs`, `list.rs`, `download/{start,pause,resume}.rs`
and `question.rs` at this level, none of which have been here since the
handlers were grouped.

| Path | Serves |
|------|--------|
| `model/` | `gglib model …` — add, list, inspect, explain, remove, capabilities, and `download/` |
| `inference/` | `serve`, `proxy`, `chat`, and `agent_question` (`gglib q`) |
| `config/` | `gglib config …` — `settings/`, `paths.rs`, `llama*.rs`, `check_deps/`, `fast_downloads.rs` |
| `agent_chat/` | The interactive agent REPL behind `gglib chat` |
| `daemon/` | `gglib daemon run / status / stop` |
| `up/` | `gglib up` — the one-command setup path |
| `proxy_dashboard/` | `gglib proxy dashboard` — the live terminal view |
| `run/` | `gglib run start / list / show / cancel` — replies the daemon owns until they end |
| `remote/` | `gglib remote enable / disable / status / invite / list / forget / join / disconnect / key` — the tunnel that puts one machine's proxy on another (ADR 0012) |
| `benchmark.rs`, `benchmark_verdicts.rs` | `gglib benchmark …`, including `tune`; the second holds the agentic report's three verdict blocks |
| `mcp_cli.rs` | `gglib mcp …` |
| `attachment.rs` | `gglib attachment save` — a stored image written to a file, found by the start of its id |
| `history.rs`, `web.rs`, `gui.rs`, `completions.rs`, `proxy_cache_clear.rs`, `proxy_trips.rs` | One command each |

Each directory carries its own README with the detail; this table exists so a
newcomer can find the right one, and so it stays true by being short.

<!-- module-docs:end -->
