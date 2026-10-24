# ADR 0024 — The tunnel opens MCP servers by name: a paired device's runs are offered the named servers' tools, and `/mcp` is closed to the tunnel

- **Status:** Accepted
- **Date:** 2026-10-23
- **Depends on:** [ADR 0012](0012-the-remote-tunnel.md),
  [ADR 0013](0013-the-target-is-a-value.md),
  [ADR 0018](0018-gglib-draws-with-stable-diffusion-cpp.md),
  [ADR 0023](0023-a-chats-folder-is-where-gglibs-own-file-tools-read.md)
- **Supersedes:** the switch
  [ADR 0012, decision 5](0012-the-remote-tunnel.md#5-mcp-is-refused-over-the-tunnel-unless-asked-for)
  chose, `gglib remote enable --allow-mcp` opening `/mcp` to tunnelled
  requests; and the rule ADR 0013's 2026-09-30 amendment set, that a
  device's turn calls no tool until that switch is on and then only the
  tools the chat's settings name. Nothing else in either.
- **Superseded by:** nothing
- **Log:** [log-0024](log-0024.md)

> Files and functions are named as they stand at the commit
> `feat(proxy): the drawing answer names the MCP servers a device's run is
> offered`, which lands this ADR. No line numbers are cited.

## Context

The owner wants web search from his phone. His Mac runs a Tavily MCP
server already, and the phone's chats already run through the Mac's agent
loop when they draw: a chat the phone keeps itself goes as
`PUT /v1/runs/{id}?kind=chat&tools=builtin`, a turn on one of the Mac's
chats as `?kind=agent`, and both show a line per tool called and keep each
call and answer. What stopped it was the grant. The daemon offered a
device's run the image tool and nothing else, and the one grant for MCP
over the tunnel was a yes/no switch, `--allow-mcp`, that opened the `/mcp`
gateway to every server configured on the Mac. A Tavily switch was a shell
server's switch too.

Facts the decision rests on:

- **Two naming rules.** The agent loop names an MCP tool
  `{numeric server id}:{tool}` (`McpToolExecutorAdapter::list_tools`,
  `crates/gglib-mcp/src/tool_executor.rs`); the `/mcp` gateway names it
  `{server name}__{tool}`. A chat's own tool list may hold a bare name,
  which the tool filter matches against that tool on every server
  (`crates/gglib-core/src/ports/tool_executor_filter/filtered.rs`). A
  grant by server name therefore has to be resolved to qualified ids when a
  run starts, and a chat's list may only narrow it.
- **Only a running server's tools are listed**
  (`McpService::list_all_tools`), and the daemon starts only eager servers
  at boot. A lazy server named for a device has to be started for its run.
- **The stored switch is one JSON row** (`RemoteServe`,
  `crates/gglib-core/src/settings_remote.rs`) inside a settings record read
  strictly, with no `deny_unknown_fields`. A new key reads an old
  `{"allowMcp": true}` as no server opened; reusing `allowMcp` for a list
  would break every settings load.
- **A phone's run without Draw already goes through the loop.**
  `?kind=chat&tools=builtin` without `draw` ran with no tool, so offering
  tools needs no new query word. Every loop run takes one of the Mac's four
  agent-loop slots (`max_concurrent_agent_loops`).
- **ADR 0012 said a per-device `/mcp` grant is a new decision**, in the
  2026-09-11 amendment to decision 5, not a refinement of that one. This
  grant is that new decision, and it is per server, for every paired device,
  not per device.

## Decision

### 1. `--allow-mcp` names servers, and the list is stored, re-armed and checked

`gglib remote enable --allow-mcp <server>` is repeatable and names the MCP
servers whose tools a paired device's runs may use, by the name `gglib mcp
list` shows. The list is stored as `RemoteServe.mcp_servers` (key
`mcpServers`), re-armed when the daemon starts, and held by the live
session's gateway (`RemoteGateway`, read through
`RemoteGatewayPort::mcp_servers`), empty with the tunnel down. It is checked
when the tunnel is enabled, by `McpService::refuse_unopenable`
(`crates/gglib-mcp/src/opened.rs`): a blank name, `builtin`, a name no
server has and one two servers share are refused, `400`, and nothing is
armed. A resume and a re-arm do not check it; a name that no longer opens
one server resolves to nothing when a run starts. An `allowMcp` an older
build stored is not read and opens nothing. Changing the list is `disable`,
then `enable` with the new names.

### 2. `/mcp` is refused to every tunnelled request

`mcp_tunnel_guard` (`crates/gglib-proxy/src/remote/mcp_guard.rs`) answers
every tunnelled request to `/mcp` with `403 mcp_not_allowed_over_tunnel`,
whatever the list names, and asks the tunnel's owner nothing. A local
request is not gated. The marker mechanism is as ADR 0012 decision 5 has
it: restrictive only, so forging it buys this refusal.

### 3. Both of a device's doors offer the named servers' tools, resolved when the run starts

`crates/gglib-axum/src/handlers/agent/device_tools.rs` says what a device's
run is offered, for a turn on the Mac's chats (`hub_turn`) and a chat the
phone keeps (`chat_turn`) alike:

- **The floor**: the image tool, as `builtin:generate_image`, for a message
  sent with Draw, whatever the chat's `no_tools` says
  ([ADR 0018, decision 10](0018-gglib-draws-with-stable-diffusion-cpp.md#10-draw-overrides-a-chats-no_tools-for-that-message));
  else nothing.
- **Beyond it**, the tools of the servers the list names, each by its
  qualified `{id}:{tool}`: every one of them for a phone's chat and for a
  Mac chat whose settings name no tool; for a Mac chat that names tools,
  only the opened tools it names exactly or by bare name; none when its
  tools are off. A tool the chat names that no opened server has is never
  added.
- **When**: a `PUT` is planned with the floor alone, and the rest is
  settled in the run's late step (`hub_turn::late_on`), once the model is
  found or loaded. There `McpService::opened_tools` reads the servers once,
  starts a named server that is not running (its executable resolved
  first, then the start the per-server lock keeps to one), and lists its
  tools through the executor's own naming rule (`qualified`). A server
  that is disabled, invalid, manual and stopped, or fails to start is
  skipped with one warning that names it and why, which for one that failed
  to start can hold what it printed on stderr, as the daemon's own start
  warning does; never a tool's arguments or results. The run goes on with
  the rest. A refused `PUT` starts nothing.
- **Never file tools**: a device's run has no folder
  ([ADR 0023, decision 4](0023-a-chats-folder-is-where-gglibs-own-file-tools-read.md#4-a-paired-devices-turn-never-has-a-folder)),
  and the list cannot name `builtin`. Never every tool.
- **Tunnelled or direct alike**: both doors admit only a named device
  (`crates/gglib-proxy/src/runs/turn.rs`), and the list is read from the
  gateway, not from the request.

The tool filter's bare-name match now ignores a bare form that itself holds
a `:`, so a tool named `5:search` on server 7 is not server 5's `search`,
and an entry that is one tool's qualified name admits that tool alone.

A replayed call in a phone's history is read by its shape, never its name:
a call to a server no longer opened, or never run, is history, not a grant.

### 4. The proxy's drawing answer names the servers

`GET /v1/images/drawing` on the proxy answers `DeviceDrawing`
(`crates/gglib-proxy/src/remote/offer.rs`): the drawing answer's fields,
flattened, and `mcp_servers`, the names the live session opens, `[]` with
none or no tunnel, present also when `available` is false. A phone reads it
on open already; it is the fact a phone acts on to send a question with
`tools=builtin` though it does not draw. An older gglib sends no key, and a
phone sends it nothing new. The daemon's `GET /api/images/drawing` does not
carry it.

## Consequences

What a person will notice:

- `gglib remote disable`, then `gglib remote enable --allow-mcp tavily`,
  and a question from the phone can search: the reply shows a line per tool
  called. `gglib remote status` says `MCP:       tavily`.
- A machine enabled with the old switch comes back with no server open, and
  `gglib remote enable` says how to open one.
- A server deleted and added again under the same name is open again; one
  renamed is not. The grant is the name, resolved when each run starts.

Costs, stated plainly:

- Every such run takes one of the Mac's four agent-loop slots, which a
  device's runs share with the page's chats and runs, those that draw
  included. While four run at once, a fifth is refused `429 agent_busy`
  rather than queued, as a run that draws is today.
- A loop run does not read the body's sampling or `max_tokens`, only
  `reasoning_budget_tokens: 0`, and applies the Mac's iteration,
  loop-detection and stagnation limits, where a plain run honoured the body.
- The named servers' tools are offered to whatever model the phone names.
  `/v1/models` carries no tool-support entry yet, so a model that cannot
  call tools is offered them anyway and may answer oddly.
- The first run after a named lazy server stopped waits inside the run for
  it to start.

## Kill criteria

1. **A client the owner means to serve needs `/mcp` itself.** The reading
   is the daemon's log, `<data folder>/logs/gglib.log.<date>`, where the
   guard's warning is written at the default level:
   `grep -h "refused a tunnelled request to /mcp" <data folder>/logs/gglib.log.*`,
   each line naming the peer. Lines from a peer the owner meant to serve, a
   client that drives the gateway rather than a run, would reopen `/mcp` per
   server as a new decision. A zero is recorded beside `gglib remote
   status`'s `Requests:` count over the same days.

## Out of scope

- **Per-device and per-tool grants.** The list opens a server to every
  paired device, and every tool of it.
- **Changing the list on a live session.** Still `disable`, then `enable`;
  the ticket and the device keys survive.
- **A device's run waiting for a free agent-loop slot** instead of `429`
  while all four are taken, and a model's tool support in `/v1/models`.
- **The `/mcp` gateway's own posture for local clients**, which ADR 0012
  decision 2 has.
