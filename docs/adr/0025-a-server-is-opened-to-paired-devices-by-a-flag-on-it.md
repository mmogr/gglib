# ADR 0025 — A server is opened to paired devices by a flag on it, set at any time, and the tunnel only says whether a session is live

- **Status:** Accepted
- **Date:** 2026-10-25
- **Depends on:** [ADR 0012](0012-the-remote-tunnel.md),
  [ADR 0024](0024-the-tunnel-opens-mcp-servers-by-name.md)
- **Supersedes:** the list
  [ADR 0024, decision 1](0024-the-tunnel-opens-mcp-servers-by-name.md#1---allow-mcp-names-servers-and-the-list-is-stored-re-armed-and-checked)
  chose: `gglib remote enable --allow-mcp <server>` naming the servers,
  stored as `RemoteServe.mcp_servers`, held by the session's gateway and
  checked at enable. Nothing else in it.
- **Superseded by:** nothing
- **Log:** none

> Files and functions are named as they stand at the commit
> `docs: ADR 0025 and the docs a server's own grant changes`, which lands
> this ADR. No line numbers are cited.

## Context

Under ADR 0024 the MCP servers a paired device's runs may use were a list
of names handed to `gglib remote enable --allow-mcp`, stored apart from the
servers in `RemoteServe.mcp_servers`, and held by the tunnel's gateway for
the session. Three things followed from where the list lived:

- **It changed only with the session.** Opening one more server meant
  `disable`, then `enable` with every name typed again, which dropped every
  device's connection. The GUI's boxes were shown only while remote access
  was off.
- **It was bound to names.** A renamed server quietly lost its grant, and a
  server deleted and added again under the same name quietly regained it.
- **Code existed only to cope with the binding**: the enable-time name
  check (`refuse_unopenable`), the "did `--allow-mcp` take?" notes in the
  CLI and the GUI, and the list threaded through enable, resume and re-arm.

The owner wants the grant easier to change, from the CLI and the GUI alike,
at any time.

## Decision

### 1. The grant is a flag on the server's own row

`mcp_servers.allow_remote` (`INTEGER NOT NULL DEFAULT 0`) is the grant, on
`McpServer` and, read only, on `McpServerDto`. Nothing else lists the
servers opened to devices. It is not on the add or edit requests, so an
edit saved from a form never changes it.

### 2. The flag has one write of its own

`McpServerRepository::set_allow_remote` writes the column and nothing else,
and has no default implementation. The whole-row writes (`insert`,
`update`) never name the column, so a write made from a copy read before a
deny cannot undo it. `McpService::set_allow_remote`
(`crates/gglib-mcp/src/opened.rs`) neither stops the server nor rewrites its
row. Opening an SSE server is refused (`SseNotSupported`), and so is opening
one named `builtin` (`NeverOpened`), with a sentence of its own. Closing is
never refused.

### 3. One read says what is open

`McpService::open_to_devices` is the servers that are flagged, enabled,
stdio and not `builtin`; `device_server_names` is their names, sorted, each
once, and none with a warning when the servers cannot be read.
`opened_tools` offers a device's run their tools, starting one that is not
running, as decision 3 of ADR 0024 has it.

### 4. The tunnel says only whether a session is live

`RemoteGatewayPort::serving` replaces `mcp_servers`. `RemoteGateway` sets it
when a session begins and clears it when that session is reset, under the
epoch ADR 0012 already guards the session with.

### 5. Every reader asks whether a session is live, then the MCP service

A device's run (`device_tools::resolve` and `settle`) and the proxy's
drawing answer (`DeviceDrawing::of`) ask `serving` first and read the open
servers only when it says yes. `gglib remote status` names the open servers
only while its tunnel is up, and the enable and invite answers, which are
given on a live session, name them as they stand. With no live session a
device is offered no server's tools, and status names none.

### 6. The list and its plumbing are gone

`RemoteServe.mcp_servers`, the enable body's `mcp_servers`, the enable-time
check, the "did not take" notes and the GUI's ticked list are deleted. An
older client's `mcp_servers` or `allow_mcp` in the enable body is read and
opens nothing. `gglib remote enable --allow-mcp` is kept hidden only to
refuse, before the daemon is asked anything, saying the flag is gone and
pointing to `gglib mcp allow-remote <names>`.

### 7. The upgrade keeps open what was open, and never opens more

`user_version` step 4 (`crates/gglib-db/src/setup_remote_grant.rs`) copies
the old list onto the rows once. It copies only when the stored
`remote_enabled` is `true`, and for each listed name only the one stdio
server that has it, never `builtin`. The two settings rows are read as raw
JSON, so a list the struct no longer has, and another setting this build
cannot read, change nothing. Rows that cannot be read copy nothing, and the
step is stamped either way.

The flag is set by `gglib mcp allow-remote` and `deny-remote`, by
`PUT /api/mcp/servers/{id}/remote` with `{"allow": bool}`, and by one GUI box
(`AllowRemoteCheckbox`) shown on each MCP panel row and as the Remote
popover's checklist. All of them call `McpService::set_allow_remote`.

## Consequences

What a person will notice:

- A change applies from a device's next run, with no restart and no
  re-enable. The phone shows the new list on its next chat open, when it
  asks the drawing answer.
- A renamed server keeps its grant. A server deleted and added again starts
  closed.
- Two servers of one name can each be open; the CLI asks for an id when a
  name is shared.
- `enable` no longer resets anything about the servers, and `disable` does
  not close them: with the tunnel down nothing is offered, and they are
  offered again when it comes back.
- The Remote popover's checklist shows whether remote access is on or off.
- A machine upgraded with its tunnel on keeps the servers its list named
  open; one upgraded with it off starts with none, as its next `enable`
  would have left it.

Costs, stated plainly:

- The drawing answer reads the servers from the database at each ask, where
  ADR 0024 read the session's list in memory. A device's run read the
  database at its start already.
- A server opened to devices is offered to every paired device, as before.

## Kill criteria

1. **Opened servers that a run finds not running, or cannot read, often
   enough to need a pushed event or an eager start.** The reading is the
   daemon's log, `<data folder>/logs/gglib.log.<date>`, where
   `opened_tools` writes one warning per server it skips:
   `grep -h "an opened MCP server is not running\|an opened MCP server's tools could not be listed" <data folder>/logs/gglib.log.*`.
   None yet: no reading has been taken since this change.

## Out of scope

- **Per-device and per-tool grants**, as in ADR 0024.
- **`/mcp` over the tunnel**, which ADR 0024 decision 2 still refuses.
- **Pushing a change to a connected phone.** It reads the list when it next
  asks.
