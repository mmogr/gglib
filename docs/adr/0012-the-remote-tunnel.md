# ADR 0012 — The remote tunnel: one key at two doors, a code that dies on use, and a ticket that dies with the session

- **Status:** Accepted
- **Date:** 2026-09-05
- **Depends on:** [ADR 0008](0008-two-binaries-one-daemon.md)
- **Supersedes:** nothing
- **Superseded by:** nothing

## Context

The hardware does not travel. A desktop with enough VRAM to run the models
gglib exists to run is a machine you leave at home, and the practical
consequence, stated in [#963](https://github.com/mmogr/gglib/issues/963), is
that the person who built the local setup ends up on a proprietary endpoint
for most of the hours in a day. The models are there; the laptop is here.

The ordinary fix is a mesh VPN, and it was rejected for what it costs rather
than for whether it works. The requirement behind #963 is sovereignty: no
account with a third party, no VPN profile installed on a client machine, and
nobody in the path who can read the traffic. Each candidate fails one of
those.

| Candidate | Why it lost |
|---|---|
| Tailscale / mesh VPN | An account, and a system-level VPN profile on every device that wants access. iOS permits one VPN at a time, so this competes with whatever else the user runs. |
| Cloudflare Tunnel | TLS terminates on their edge. The traffic is readable by someone who is not us, which is the property #963 exists to avoid. |
| Port forwarding | A public listener with no auth in front of it. January 2026's survey of 175,000 publicly exposed Ollama servers is what this looks like at scale. |

What remained was a peer-to-peer transport with no server in the middle, and
`modelpipe` — the crate, not the CLI — is that: two endpoints dial out to a
relay that introduces them, hole-punch a direct encrypted QUIC connection, and
fall back to the relay only when the punch fails, at which point the relay
carries ciphertext it cannot read. The keys are the identities. There are no
certificates and no certificate authority.

Embedding it exposed a trap that is the real subject of this ADR. The tunnel
delivers remote traffic to loopback, and gglib's proxy treats loopback as
trusted in two separate places at once. `host_guard` in
`crates/gglib-proxy/src/access/mod.rs` passes a loopback `Host` with no
configuration at all, which is what makes the tunnel work out of the box. And
`resolve_api_key` in `crates/gglib-runtime/src/proxy/supervisor.rs` returns
`(None, ApiKeySource::None)` for a loopback bind — deliberately, so that a
local-only proxy is not ceremony — which means `bearer_guard` is installed
over a policy that admits everyone. Key generation triggers on a non-loopback
bind, and the bind never changes when the tunnel is what moved.

So the naive embedding hands a ticket-holder the whole loopback trust surface
unauthenticated, including the MCP gateway. That is not a defect in either
guard. It is the two of them agreeing on a premise — that reaching loopback
means being on this machine — which the tunnel makes false.

## Decision

### 1. The tunnel lives in the daemon, not in the CLI

gglib depends on the `modelpipe` crate and drives it from
`crates/gglib-app-services`, as a `RemoteOps` sitting beside the existing
`ProxyOps` in the service graph. The CLI's `gglib remote` subcommands are thin
clients over the daemon's HTTP API, which is what ADR 0008 says every client
is.

Running the tunnel inside the CLI process was the obvious shortcut and it
loses on two counts. The desktop app could not drive it at all — there would
be nothing for a Remote toggle in the GUI to toggle — and a listener that
lives for the length of a foreground command has nothing to receive a key
rotation. Decision 2 needs a long-lived object to call `set_token` on. Put
another way, the placement is not a preference about layering; it is a
precondition for the credential model below.

### 2. One credential, checked at two doors

modelpipe's serve side is constructed with
`TokenPolicy::Supplied(proxy_api_key)` — the same bearer token the gglib proxy
enforces. A request carrying the wrong token is refused at the tunnel edge,
before a byte reaches the daemon, and would be refused again by the proxy's
own `bearer_guard` if it got there. The second check is not redundant: it is
what keeps the local and remote paths from diverging into two auth stories.

The consequence has to be stated plainly, because it changes behaviour for
setups nobody touched. Since `resolve_api_key` mints nothing on a loopback
bind, most installations have no `proxy_api_key` at all. So **`gglib remote
enable` force-generates and persists `proxy_api_key` when none is set**, and
because the proxy is one listener, that puts a bearer requirement on the
*local* loopback proxy too. gglib's own CLI and GUI read the key from settings
and carry on; a third-party local client configured by hand against an open
loopback proxy starts getting 401 the first time the tunnel is enabled.

We considered a second listener bound only for tunnelled traffic, so the local
proxy could stay open. It doubles the port surface, doubles the guard wiring,
and buys a property — an unauthenticated local endpoint on a machine that is
now reachable from outside it — that is not worth defending.

Authentication turns on and never off. `BearerPolicy::tracking` in
`crates/gglib-core/src/access/bearer.rs` keeps the bind-time token as a floor
precisely so that clearing the stored value cannot silently reopen a listener,
so `gglib remote disable` stops the tunnel and leaves the key in place. That
is the floor working, not an oversight.

Rotation had no mechanism and needed one. There is no settings-changed event
in gglib and there cannot be a useful one: `gglib config settings set` writes
the same SQLite file from a different process, which is the reasoning
`SettingsCache` already records. So `RemoteOps` polls `BearerPolicy::current()`
on the settings-cache cadence — `SETTINGS_CACHE_TTL`, 5 s — and calls
`ServeHandle::set_token` when it changes. Staleness is bounded rather than
zero, on the same terms the proxy already accepts. A key supplied by
`--api-key` or `GGLIB_API_KEY` produces `BearerPolicy::pinned`, and a pinned
key is never overridden by anything in settings; the poller respects that
rather than working around it.

### 3. Pairing moves a one-time code, not the key

`gglib remote enable` prints the ticket and a six-digit numeric code. The code
lives 120 seconds, is spent on first use, and is burned after three wrong
attempts. Under it, the desktop calls `ServeHandle::grant_once(code, 120s)`,
which admits exactly one request bearing that code through the tunnel edge
without the bearer token.

The laptop POSTs `{"code": "..."}` to `POST /v1/remote/pair` on the proxy. That
route sits **outside** the bearer-guarded group in
`crates/gglib-proxy/src/server.rs` — it cannot require the credential it
exists to hand out — and **inside** the host allowlist, which is applied
outside the router and therefore covers it. The response carries the real
`proxy_api_key` over the encrypted hop, and the laptop stores it in its own
settings as `remote_api_key`. Every failure — wrong code, expired code, spent
code, malformed body — is a flat 401 `invalid_pairing_code`, which tells an
attacker only that they did not get in.

Six digits is about 20 bits, which is not much on its own. It is defended by
three things together: the three-attempt burn, the 120-second window, and the
fact that reaching the route at all requires the ticket, which carries an
endpoint id nobody can guess. Guessing the code without the ticket is not a
slower attack, it is a different one.

Rejected: **bundling the key into the pairing string.** It makes the printed
string a standing credential, so a photograph of the screen — or a screen
share, or a scrollback — is durable access rather than a 120-second window.
Also rejected: **keeping ticket and token separate**, as #963 first proposed
and as the modelpipe CLI does. That is correct for a command-line tool with
two outputs going to two places. For a person on a train it means moving two
long opaque strings by hand every session, and the second one is the one that
matters most.

### 4. A fresh identity every session

The serve side is built with `identity: None`, so every `enable` mints a new
endpoint key and therefore a new ticket. `enable` is never persisted and never
auto-starts on daemon launch. A laptop that has already paired re-pairs with
the ticket alone, since it holds the key.

The alternative is modelpipe's `--identity`: a stored endpoint key in gglib's
data directory, so the ticket survives restarts. modelpipe's own
[ADR 0002](https://github.com/mmogr/modelpipe/blob/main/docs/adr/0002-a-stored-endpoint-key-opt-in.md)
records the trade, and here it comes out the other way. A ticket is bearer
material. A leaked one against a stored identity is good until someone deletes
a file they have to remember exists; against a fresh identity it is dead at
the next restart, and restarting is something people do anyway. Free
revocation is worth more to this feature than saved typing.

The cost is real and accepted: the ticket string has to reach the laptop every
session. The pairing string is `<ticket>-<code>` — base32 tickets never contain
`-`, so the split is unambiguous, and QR alphanumeric mode uppercases the
whole thing, which the ticket format tolerates by parsing
case-insensitively.

### 5. `/mcp` is refused over the tunnel unless asked for

Tunnelled requests get 403 `mcp_not_allowed_over_tunnel` on `/mcp` by default.
`gglib remote enable --allow-mcp` turns it on.

The mechanism is the two headers modelpipe sets on every forwarded request
after stripping any inbound copies: `Via: 1.1 modelpipe`, and
`X-Modelpipe-Peer` carrying the twelve-character peer fingerprint. A gglib
proxy middleware reads them into a request extension, and a `route_layer` on
`/mcp` alone refuses when the extension says tunnelled and the flag is off.

The marker is restrictive only, which is what makes it safe to act on. A local
client that forges the headers denies itself `/mcp` and increments a counter.
A tunnelled peer cannot remove them, because the serve side overwrites rather
than inherits. Neither direction of forgery grants anything.

Why this route and not others: `invoke_tool` in
`crates/gglib-proxy/src/mcp/handlers.rs` starts and drives the MCP server
processes configured on the desktop. If one of those is a shell or filesystem
server — which is the ordinary reason to configure one — then a leaked bearer
token is remote code execution on the machine at home, not merely free
inference on it. The blast radius of the two is not comparable, so they do not
get the same default.

### 6. What the network learns

Port mapping (UPnP/NAT-PMP) is off by default. It costs nothing that matters:
pairing works either way, and a few NATs fall back to the relay slightly more
often. Asking the router unprompted is not a thing this should do quietly.

`--relay` is exposed on both sides, so anyone who would rather not use n0's
relays can run their own. `--no-discovery` is exposed too, as an advanced flag
that warns, because it removes a property people will assume they still have:
with discovery on, a ticket keeps working after the serving machine changes
network, and with it off the ticket carries only the paths it was minted with.

What is contacted regardless, and it is not nothing: n0's discovery service
learns both endpoint ids and the IP address each publishes from, refreshed
every few minutes for as long as the tunnel runs. A relay, when one is used,
learns the pair of endpoint keys, both IP addresses, and the timing and volume
of traffic. It never learns content. Observability is not readability, and
saying so is better than implying the number of observers is zero.

### 7. The connect side, and the door that only opens from inside

The laptop's connect side runs in the laptop's own daemon, on its own loopback
port beside that machine's local proxy. Both ends of this feature are daemon
concerns for the same reason.

That listener does **not** inject `Authorization`. gglib's own `q` and
`chat --remote` attach the stored `remote_api_key` themselves, and a
third-party client pointed at the port supplies the key as its API key, which
is the ordinary OpenAI-compatible arrangement. A listener that injected
credentials would make every process on the laptop an authenticated client of
the desktop, which is a larger grant than the one the user made.

For the kill switch, local `gglib remote disable` stops the tunnel. Remotely,
`gglib remote kill` posts `{"confirm":"shutdown"}` to `POST
/v1/proxy/shutdown` through the tunnel — an existing route, already inside the
bearer-guarded group, whose confirmation body is already required by
`crates/gglib-proxy/src/admin.rs`. Nothing new is exposed. This is a one-way
door: it cancels the daemon's own shutdown token, so the proxy, the model
servers and any downloads stop together — not merely the tunnel, which would
leave the models loaded and the machine still answering on its LAN — and
nothing can restart any of it until someone is at the machine. That asymmetry
is correct. A remote start would be a remote start, and there is no version of
it that is only available to the right person.

## Consequences

**Good:**

- Remote access with no account, no VPN profile on the client, and no third
  party that can read a request. The relay sees ciphertext or is not in the
  path at all.
- One credential to reason about. The token that gets a request through the
  tunnel edge is the token the proxy checks, so there is one answer to "what
  is my key" and one place to rotate it.
- A ticket alone is useless, and it expires when the session does. A code
  alone is useless, and it expires in two minutes or on first use.
- The GUI gets a Remote toggle for free, because the logic is daemon-side.

**Costs, accepted:**

- The ticket has to be moved to the laptop every session. This is the direct
  price of decision 4 and the thing most likely to be re-litigated; the
  counter-argument is in modelpipe's ADR 0002 and it is not weak.
- `--no-discovery` and a per-session identity interact badly by construction:
  a ticket that carries only its minting addresses, from an endpoint that will
  not exist next time, is nearly useless. Both flags are documented; the
  combination is not recommended.
- Rotation is eventually consistent within 5 s. A revoked key keeps working at
  the tunnel edge for up to one settings-cache window.

**Stated plainly, because it will surprise people:**

- **Enabling remote access puts a bearer requirement on the local proxy**, and
  disabling it does not take that away. gglib's own clients recover by reading
  settings. A hand-configured local client — a browser UI pointed at
  `127.0.0.1:8080/v1` with no key — begins getting 401 and needs the key
  added once. This is the intended behaviour and there is no flag to opt out
  of it, because the alternative is an unauthenticated endpoint on a machine
  that is now reachable from outside.
- `/mcp` over the tunnel is off even for a correctly authenticated peer with
  the right key. It is a separate grant because it is a separate blast radius.

## Out of scope

Named here so that their absence reads as a decision rather than an oversight.

- **A phone client.** iroh compiles to wasm and would run relay-only, still
  end-to-end encrypted. That is a product, with its own surface and its own
  release story, not a flag on this one.
- **LAN and mDNS pairing.** gglib already carries `mdns-sd` in the CLI, so
  discovering a desktop on the same network without moving a ticket is
  plausible. It is a different trust model — presence on a network as
  evidence — and it needs its own argument.
- **Per-peer pinning of the pairing request.** `grant_once` admits one request
  bearing the code from whichever peer arrives first. Binding it to a
  fingerprint the desktop has not yet seen is circular, and solving that needs
  a second exchange nobody has designed.
- **Federation.** One pipe, one backend, matching modelpipe's own non-goals.
  Multiple desktops behind one ticket, or routing between them, is not a
  bigger version of this feature.
