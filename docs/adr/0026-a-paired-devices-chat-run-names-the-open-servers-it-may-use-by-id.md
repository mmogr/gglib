# ADR 0026 — A paired device's chat run names, by id, the open servers it may use, and no other is started

- **Status:** Accepted
- **Date:** 2026-10-26
- **Depends on:** [ADR 0018](0018-gglib-draws-with-stable-diffusion-cpp.md),
  [ADR 0024](0024-the-tunnel-opens-mcp-servers-by-name.md),
  [ADR 0025](0025-a-server-is-opened-to-paired-devices-by-a-flag-on-it.md)
- **Supersedes:** the `tools=builtin` word that
  [ADR 0018, decision 13](0018-gglib-draws-with-stable-diffusion-cpp.md#13-mcp-reaches-the-tool-only-behind-a-switch-that-is-off)
  chose and
  [decision 9](0018-gglib-draws-with-stable-diffusion-cpp.md#9-generate_image-is-offered-only-to-a-message-sent-with-draw)
  names; for a phone-kept chat, the every-open-server offer of
  [ADR 0024, decision 3](0024-the-tunnel-opens-mcp-servers-by-name.md#3-both-of-a-devices-doors-offer-the-named-servers-tools-resolved-when-the-run-starts);
  and the names-only `mcp_servers` of
  [decision 4](0024-the-tunnel-opens-mcp-servers-by-name.md#4-the-proxys-drawing-answer-names-the-servers).
  Nothing else in either.
- **Superseded by:** nothing
- **Log:** none

> Files and functions are named as they stand at the commit
> `docs(adr): 0026, a paired device's chat run names the open servers it may use by id`,
> which lands this ADR. No line numbers are cited.

## Context

Under ADR 0024 and ADR 0025 a chat a phone keeps itself had one switch for
the Mac's tools. `tools=builtin` offered the tools of every server open to
paired devices; `tools=none` offered none of them. A person who opened a
search server and a file server to their phone could not ask one question
with search alone: every question that used tools started every open
server.

ggchat now lets each conversation pick which of the Mac's open servers it
uses. The Mac stays the ceiling: a phone can only narrow what the Mac
opens, never widen it. That needs a way for a run to name servers, and a
way for the phone to learn what it may name.

Servers already have a stable identity. `McpServer.id` is an
`AUTOINCREMENT` key, so an id is never reused, and it reaches the phone
already, in each tool's qualified name `{id}:{tool}`. Names are unique only
by the service's own rule: a database from before that rule can hold two
servers of one name, and a rename changes the name and keeps the id.

## Decision

### 1. A server is named by its id, and the drawing answer gives the id beside the name

The proxy's `GET /v1/images/drawing` lists `mcp_servers` as `{id, name}`
objects (`DeviceDrawing`, `crates/gglib-proxy/src/remote/offer.rs`), one per
server open to paired devices, in order of name and then id, read from
`McpService::device_servers`. Two servers sharing a name are each listed.
Only this answer changes: the status, enable and invite answers, the CLI,
the TypeScript bindings and the daemon's `GET /api/images/drawing` keep
names.

### 2. `tools` is `none` or the ids, and `builtin` is gone

`PUT /v1/runs/{id}?kind=chat&tools=<T>`, where `<T>` is `none` or ids in
canonical decimal joined by commas (`3,7`), read into a set (`ToolsWord`,
`crates/gglib-proxy/src/runs/handlers.rs`). `builtin` is refused. The set
goes in `tools` rather than in a key of its own because the query does not
deny unknown keys: a new key a gglib did not know would be dropped and the
run would fail open, where an unknown `tools` value is refused and fails
closed.

### 3. A malformed value is refused; a closed id is dropped where servers start

A value that is not `none` or canonical ids is `400 invalid_request`: an
empty value, an empty id, `0`, a sign, a leading zero, `none` beside an
id, the key twice, and any word. An id that is well formed but not open is
dropped, never refused: stale picks are normal, since a phone learns of a
closed server only when it next asks, and from outside a closed id and one
no server has look the same. It is dropped in `McpService::opened_tools`,
before any server starts, which is the one place a device's run starts a
server and runs after the `PUT`, when the open set may have changed. One
`warn!` names the dropped ids and nothing else. There is no second check:
the intersection only narrows.

### 4. Only the named open servers are started

A phone-kept chat run carries the set of ids its query names, from the
proxy (`AgentRunStarter::start_chat`) to the run (`Asked::of_phone`), and
nothing on that path can say every open server. The run reads it as a
`DeviceServers` (`crates/gglib-core/src/domain/mcp/device.rs`), `All` or
`Only(ids)`, whose `All` only a Mac-kept chat's turn reaches and whose
default admits none, so a field nobody set offers nothing. The run is
offered the tools of the named servers that are open, and an open server it
does not name is neither started nor offered.

### 5. Draw is independent of the servers

With `draw=true` the run is offered `builtin:generate_image` whichever
servers it names, `none` included, as ADR 0018 decision 10 has Draw
override a chat's tools being off.

### 6. A Mac-kept chat, the status answers and the CLI are unchanged

A turn on a chat the Mac keeps (`kind=agent`) is still offered every open
server, narrowed by the chat's own tool list, and `tools` beside it is
still refused. The Mac's own chats keep `no_tools` and `--no-tools`.

### 7. A `PUT` under a started run's id keeps its tools

The run already started answers before its body is read as a chat request
(`chat_turn::start`), so a phone that puts a question again with other
picks gets the run it has, with the servers it started with. Its query is
read first, at the proxy, so a malformed one is still refused there. A change of
picks applies from the next question.

## Consequences

What a person will notice:

- A phone can ask one question with one server, and the others stay
  stopped.
- A server closed on the Mac since the phone last asked is quietly left
  out; the run answers with the rest.
- A phone app from before this change (ggchat 0.11.0 and older) reads
  `mcp_servers` as a list of names, so it cannot read a drawing answer
  that lists any server: while a server is open it shows Draw as unknown
  and sends plain chat runs, with no tools, until it is updated. Only a
  drawing answer it read before the desktop was updated leads it to send
  `tools=builtin`, which is refused with `400 invalid_request`.

Costs, stated plainly:

- Ids are per Mac. A phone's picks mean nothing on another Mac, and
  re-pairing drops them on the phone's side.
- A database reset renumbers servers from 1, so a pick kept on a phone can
  name a different server. The Mac's open set still bounds it.

## Kill criteria

1. **The warning appears on most device runs.** Picks would then be stale
   more often than not, and refusing or pushing the open set would serve
   better. The reading is the daemon's log, `<data folder>/logs/gglib.log.<date>`:
   `grep -hc "named MCP servers that are not open to paired devices" <data folder>/logs/gglib.log.*`,
   set against the questions sent from the phone in those days, counted in
   its own chats. None yet: no reading has been taken since this change.
2. **Someone asks for per-tool choice.** An issue on the tracker asking to
   pick a server's tools one by one, not the server.
3. **A database reset under paired phones.** `gglib mcp list` showing ids
   lower than those a phone already picked means its picks may name other
   servers; then names, or a reset that forgets picks, would be needed.

## Out of scope

- **Per-tool picks.** A server is picked whole.
- **Ids on the status, enable and invite answers**, which keep names.
- **Choosing servers for a turn on a Mac-kept chat**, which keeps its own
  tool list.
