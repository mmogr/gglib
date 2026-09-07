# ADR 0012 — The remote tunnel: one key at two doors, a code that dies on use, and a ticket that dies with the session

- **Status:** Accepted
- **Date:** 2026-09-05 (amended 2026-09-07 — see the dated notes under decisions 2 and 3, the note on how authentication is turned back off, the second reading, the third reading, and Out of scope)
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
`resolve_api_key` in `crates/gglib-runtime/src/proxy/api_key.rs` returns
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

Authentication turns on, and nothing turns it off by itself. `gglib remote
disable` stops the tunnel and leaves the key in settings, so a client
configured during the session keeps working afterwards.

The floor is not what holds that, and the difference is worth stating because
the code reads as though it were. `BearerPolicy::tracking` in
`crates/gglib-core/src/access/bearer.rs` keeps the bind-time token as a floor,
so a listener that bound *with* a key cannot be reopened by clearing the
stored value. A loopback proxy binds with none — that is `resolve_api_key`'s
whole point — and `RemoteOps::enable` starts the proxy before it mints the
key, so the listener that enabling closes has an empty floor. It closes
because `current()` re-reads the settings cache and finds the new value;
`enable` sleeps one cache window before handing out a ticket for exactly that
reason. Clear `proxy_api_key` before that daemon next restarts and `current()`
returns `None`, `admits()` admits everyone, and the local proxy is open again.
Restart while the key is still stored and `resolve_api_key` hands it to
`tracking` as the bind key; from then on the floor is real.

The tunnel edge does not follow it down. The rotation poller ignores a cleared
setting, so a tunnel enabled before the clear keeps demanding the token it was
given. That is the one credential and its two doors briefly disagreeing about
whether it is required — the only case where they do, and the reason the
accurate version of "on and never off" is "on, and the floor arrives at the
next restart".

> **Amended 2026-09-07 — what the operator is supposed to do about it.** The
> two paragraphs above describe the reopening as a property of the code, which
> it is, and stop there. That leaves the one question a reader arrives with
> unanswered, and `gglib remote enable` prints "authentication turns on and
> never off by itself", which invites the reader to go looking. So, plainly:
>
> - **On a proxy that binds loopback, turning it back off is `gglib config
>   settings unset proxy-api-key`, followed by rebinding the proxy** — `gglib
>   proxy stop` and start it again, or `gglib daemon stop`, which takes the
>   listener down with everything else. `settings set --proxy-api-key ""` is
>   not it and is refused — `Proxy API key cannot be blank — clear it instead
>   to disable authentication`. Emptying the field in the desktop app's
>   settings sends `null` and clears it the same way `unset` does. The
>   loopback qualifier is load-bearing; the next bullet is why.
> - **Off loopback the same procedure mints a key rather than removing one.**
>   `--host` is first-class on both commands — `ProxyBindArgs` in
>   `crates/gglib-cli/src/proxy_bind_args.rs` for `gglib proxy`, and
>   `ServeOptions` in `crates/gglib-cli/src/shared_args.rs` for `gglib serve`
>   — and a bind that is not loopback never reaches the `ApiKeySource::None`
>   return. With the setting cleared,
>   `resolve_api_key` falls past the `Settings` branch, finds
>   `is_loopback_host("0.0.0.0")` false, calls `generate_api_key()` — and
>   then **writes the result back into `settings.proxy_api_key`**
>   (`crates/gglib-runtime/src/proxy/api_key.rs`). So the rebind repopulates
>   the field the operator just cleared and the endpoint comes back closed on
>   a credential nobody has read. `gglib config settings show` prints it
>   unmasked, which is the recovery. This is deliberate — an endpoint on a
>   network is not left open — but stating the procedure without the
>   qualifier told a `--host 0.0.0.0` operator to lock themselves out, which
>   is what this bullet exists to stop.
> - **The rebind is the load-bearing half, and its order is the opposite of
>   what it looks like.** `resolve_api_key` runs at bind, in
>   `ProxySupervisor`'s start path — not at daemon start — so unsetting alone
>   works only against the listener `enable` closed, which bound on loopback
>   with an empty floor. Once a proxy has bound with the key stored,
>   `resolve_api_key` hands it to `tracking` as the bind key and unsetting no
>   longer opens anything. Unset *then* rebind; rebind then unset leaves it
>   closed.
> - **`/mcp` is inside what reopens.** It sits in the bearer-guarded group in
>   `crates/gglib-proxy/src/router.rs` like every other protected route, so an
>   unset that lands before the rebind takes the tool gateway back to
>   unauthenticated along with `/v1/*`. That is the pre-`enable` posture of a
>   loopback proxy rather than a new hole — decision 5's tunnel gate is
>   independent of the bearer and still refuses tunnelled `/mcp` — but the
>   two guards named in *Context* are both off at that moment, and this ADR
>   exists because that combination is easy to reach by accident.
> - **A running tunnel does not reopen with it.** `rotation_poll` ignores a
>   cleared setting, so the tunnel edge keeps demanding the token it was given
>   until `disable`. The clear opens the local door only.
>
> **What holds the sentences above, and what does not.**
> `clearing_reopens_a_listener_that_bound_on_loopback` in
> `crates/gglib-core/src/access/bearer_tests.rs` was described here as pinning
> both halves. It does not, and the correction belongs in the record next to
> the claim. Delete `stored.or_else(|| self.floor.clone())` from
> `BearerPolicy::current` and its *second* half fails — at
> `"after a rebind the same clear is refused"` — alongside the two tests that
> already covered exactly that,
> `clearing_the_setting_does_not_reopen_a_closed_endpoint` and
> `a_blank_stored_key_falls_back_rather_than_opening`. Its first half is not
> *uniquely* pinned either. Deleting the settings read from
> `BearerPolicy::current` fails its `:206` assertion — but fails
> `authentication_can_be_switched_on_at_runtime`, which asserts the same
> transition from the same starting policy, and
> `a_rotation_takes_effect_without_a_restart` with it. Deleting the expiry
> check from `SettingsCache::get`'s fast path fails its `:212` assertion —
> alongside those same two and `a_write_is_observed_after_the_window_expires`,
> which owns that behaviour. Each of its three assertions is reachable by some
> production deletion and none of them alone: it documents the reopening, it
> does not hold it.
>
> The mechanism the bullets above actually turn on is `resolve_api_key`, and
> that had no test module at all — which is why the missing loopback qualifier
> survived review. `crates/gglib-runtime/src/proxy/api_key.rs` now carries one:
> `a_loopback_bind_asks_for_no_token_and_stores_none` (deleting the
> `is_loopback_host` early return fails it),
> `a_non_loopback_bind_mints_a_token_and_writes_it_back` (deleting the
> write-back fails it), `a_stored_key_is_honoured_on_a_loopback_bind` (moving
> the loopback check above the `Settings` branch fails it) and
> `a_configured_key_outranks_both_the_store_and_the_host`.

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

> **Amended 2026-09-07 — the collaborator named here does not exist.**
> `BearerPolicy` appears nowhere in `crates/gglib-app-services`; nothing in
> `RemoteOps` constructs one or calls `current()`. What the poller actually
> does is read `Settings::proxy_api_key` through `AppCore` directly, on the
> same `SETTINGS_CACHE_TTL` cadence, skipping a value that is cleared or
> unchanged and calling `ServeHandle::set_token` otherwise
> (`remote/rotation.rs`). The pinned precedence this paragraph promises is
> real, and arrives from somewhere else entirely: `settle_key` asks
> `key::decide` about `ProxyOps::effective_api_key()`, and `enable` simply
> does not spawn the poller at all when the answer is pinned
> (`remote/mod.rs`). So the behaviour is what this section describes and the
> mechanism is not. Recorded rather than quietly corrected, because the next
> person to go looking for `BearerPolicy::current()` in `RemoteOps` will spend
> the search finding nothing.

### 3. Pairing moves a one-time code, not the key

`gglib remote enable` prints the ticket and a six-digit numeric code. The code
lives 120 seconds, is spent on first use, and is burned after three wrong
attempts. Under it, the desktop calls `ServeHandle::grant_once(code, 120s)`,
which admits exactly one request bearing that code through the tunnel edge
without the bearer token.

The laptop POSTs `{"code": "..."}` to `POST /v1/remote/pair` on the proxy. That
route sits **outside** the bearer-guarded group in
`crates/gglib-proxy/src/router.rs`, with its handler in
`crates/gglib-proxy/src/remote/pair.rs` — it cannot require the credential it
exists to hand out — and **inside** the host allowlist, which is applied
outside the router and therefore covers it. The response carries the real
`proxy_api_key` over the encrypted hop, and the laptop stores it in its own
settings as `remote_pairing`, bound to the ticket of the machine that issued
it — a key that outlives the machine it names is a 401 dressed up as a
pairing. Every failure — wrong code, expired code, spent
code, malformed body — is a flat 401 `invalid_pairing_code`, which tells an
attacker only that they did not get in.

Six digits is about 20 bits, which is not much on its own. It is defended by
three things together: the three-attempt burn, the 120-second window, and the
fact that reaching the route at all requires the ticket, which carries an
endpoint id nobody can guess. Guessing the code without the ticket is not a
slower attack, it is a different one.

> **Amended 2026-09-07 — the three defences are each real, and no single path
> has all three.** Read out of the code rather than reasoned about, and it
> splits by which door the guess arrives at.
>
> **Through the tunnel, the burn never fires.** `redeem.rs` sends the code
> twice on purpose — as the bearer, so the edge's one-time grant admits the
> request, and in the body, so this route can check it. That makes a *wrong*
> code a wrong bearer. modelpipe's `Credential::admits` finds it is neither
> the enforced token nor a live grant, and the edge answers with its own
> `401 invalid_api_key` without forwarding anything, exactly as decision 2
> says it should. `handle_remote_pair` therefore never runs,
> `Pairing::attempts` never increments, and no third wrong code burns
> anything. What is left guarding ~20 bits is the 120-second window and the
> ticket — two of the three — with the number of guesses bounded only by how
> many requests fit in the window. (modelpipe caps a peer at 64 concurrent
> streams, which is a concurrency budget, not a rate limit.) The burn is still
> reachable, but only by a peer already holding the real key and sending a
> wrong code in the body, which is not the attacker it was written for.
>
> **Locally, the burn fires and the ticket is not required at all.**
> `/v1/remote/pair` sits outside the bearer group, and `host_allowed` admits
> any loopback `Host` unconditionally — the same loopback trust the Context
> section is about, reaching the one route deliberately left outside the
> credential. `redeem_pairing_code` uses the peer fingerprint only for the
> event it emits, never as a condition. So any process on the serving machine
> can POST three wrong codes to `http://127.0.0.1:8080/v1/remote/pair` and
> kill a pairing that is on screen, holding neither ticket nor key; one that
> guesses right is handed the key.
>
> Decision 3 stands. The code is still not a standing credential, it still
> dies on use and in two minutes, and a tunnelled guesser still needs the
> ticket. The sentence that does not stand is "defended by three things
> together", which is true of neither path this feature actually has.

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

Why this route and not others: the `invoke_tool` arm of
`handle_meta_tools_call` in `crates/gglib-proxy/src/mcp/handlers.rs` starts
and drives the MCP server processes configured on the desktop. If one of those is a shell or filesystem
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
`chat --remote` attach the key from the stored `remote_pairing` themselves,
and a
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
- **It is the proxy and only the proxy.** The daemon's management API on
  `127.0.0.1:9887` settles its own token at bind — none at all for the
  loopback default — and a later settings write does not reach it. That is
  what keeps the CLI, the desktop app and the `gglib remote disable` that
  undoes all this reachable after `enable` runs. This ADR originally reasoned
  about the proxy and the tunnel edge and said nothing about the third
  listener, which is how a daemon came to read the proxy's key and 401 its own
  clients.
- `/mcp` over the tunnel is off even for a correctly authenticated peer with
  the right key. It is a separate grant because it is a separate blast radius.

## Kill criteria

- If `llama-server` — or another dependency gglib fundamentally relies on —
  ships a first-party remote transport, this defers to it and the tunnel is
  deleted rather than carried beside it. The reading is a survey taken at each
  pin bump, which is already a deliberate, reviewable moment:
  `PINNED_LLAMA_RELEASE` in `crates/gglib-runtime/src/llama/download/mod.rs`
  moves one commit at a time, and `gglib config llama status` prints the
  version, commit and binary path of what is actually installed — whose
  `--help` is where such a transport would announce itself. Nothing probes for
  it: `RuntimeCapabilities` records build numbers and parser behaviour, not
  transports. This is a person reading upstream at a moment that already
  exists in the process, not a counter waiting to be added.
- If the trust model itself proves unsound — the premise that a session ticket
  plus one bearer token is enough to put a machine's proxy on the network, as
  opposed to a guard that got its own rule wrong — this is withdrawn rather
  than patched. The reading is the issue tracker:
  `gh issue list --label "priority: critical" --label "component: proxy"`, and
  the same query with `component: gui`, which is the label
  `crates/gglib-app-services` carries and therefore where `RemoteOps` reports.
  The issue form offers no `component: remote`, so the query returns more than
  this feature and the judgement — premise or implementation — stays with the
  reader. Naming that gap is the point: a criterion that pretended to a label
  which does not exist would be unreadable in exactly the way ADR 0011's first
  criterion was.
- If `RemoteStatus.tunnelled_requests` stays at zero across daemon runs long
  enough that a remote session would have shown up, the tunnel is a feature
  nobody uses and it goes, taking the `modelpipe` dependency and both sides
  with it. `gglib remote status` **on the serving machine** — the one that ran
  `gglib remote enable` — is where it and `last_tunnelled_ms` are read. The
  counter ticks in the proxy that *receives* tunnel-marked requests, so the
  serving side is the only side that has one: it prints
  `Requests:  N served through the tunnel` while serving is on, and a
  `Last one:` line once one has arrived. The connecting machine prints neither,
  only a pointer to where the number lives, because a zero taken there says
  nothing about the tunnel. Both count from daemon start rather than from
  install, so the denominator is a single daemon run and a zero has to be read
  against how long that run was.

### First reading, 2026-09-06

The first evaluation of these three criteria, and it has one scope note that
outweighs every number under it: **the tunnel has never carried traffic
between two machines.** No two-machine test has been run and none exists in
the suite — `modelpipe::serve` and `modelpipe::connect` are called only from
`gglib-app-services` (`remote/mod.rs` and `remote/connect.rs`), and every test
that exercises a tunnelled request synthesizes the markers `via: 1.1
modelpipe` and `x-modelpipe-peer` against a proxy bound in-process.

So the lines below record **not yet run**, not zero.
[ADR 0010](0010-the-loop-guard-reads-what-came-back.md)'s first reading drew
this distinction for a criterion nobody had exercised — "a zero here is the
absence of the test, not its result" — and the same applies to all three here,
more strongly: writing 0 would make a feature nobody has finished testing look
like a feature nobody wants.

> **Amended 2026-09-06 — the tunnel has since carried traffic between two
> machines.** A macOS host served and a Linux machine connected the same day
> this note was written, and the session is recorded under *Second reading*
> below. It supersedes the third line only: `tunnelled_requests` now has a
> session behind it and no longer reads *not yet run*. The first two are
> unaffected — neither reading was taken that evening either, so both stay
> unread rather than clean.

- **If a dependency ships a first-party remote transport** — **not evaluated,
  and the reading has had no occasion to be taken.** `PINNED_LLAMA_RELEASE` is
  `b10327`, the value it held when this landed; the pin has not moved since.
  **OPEN, and unread rather than clean.**
- **If the trust model itself proves unsound** — **not evaluated.** The query
  above has not been run for this note, so there is no count here, clean or
  otherwise. Recorded as unread rather than reported as zero, because a zero
  taken from a query nobody ran is the failure this ADR's kill criteria exist
  to avoid. **OPEN.**
- **If `tunnelled_requests` stays at zero** — **not yet run.** Every daemon
  that has run this code reports `tunnelled_requests` 0 and `last_tunnelled_ms`
  `None`, and every one of them was a daemon no second machine ever dialled.
  This is the criterion a zero most easily misleads, and here it does not even
  reach the ambiguity: "nobody uses the tunnel" and "nobody has yet paired two
  machines" produce the same 0, and only the first would license deleting
  anything. Not actionable until a two-machine session exists to count.
  **OPEN.**

**All three remain OPEN, and none has been read against traffic.** What this
note establishes is the state the feature shipped in — reasoned through,
guarded, and never once run end to end — so that the first person to pair two
machines knows there is somewhere to put the number.

### Second reading, 2026-09-06 — the first two-machine session

The first time `modelpipe::serve` and `modelpipe::connect` have been on
opposite sides of a network. Everything above was written against a feature
that had never carried a byte between two machines; this note retires that
sentence and changes nothing else in the reading it amends.

**Scope.** One evening, one session, one paired peer. A macOS host served —
`Matts-MacBook-Pro`, ticket `de371f8d0f7f` — and a Linux machine connected,
peer `387480a0854e`, which is also the first time the two sides have run on
different operating systems. Every subcommand was exercised: `enable`,
`connect`, `status`, `q --remote`, `disconnect`, `kill`, and a second pairing
on a fresh ticket.

**It cannot be re-run.** `tunnelled_requests` and `last_tunnelled_ms` count
from daemon start, as the criterion itself says, and both daemons have since
stopped. Nothing stored the numbers, so this note is their only durable
record — the same provenance gap
[ADR 0009](0009-fit-the-context-to-the-machine.md)'s first reading names for
the ledger's counters, named here rather than dressed up.

**The answer came from the other machine, and that is shown rather than
assumed.** `gglib model list` on the connecting machine printed `No models
found` — an empty catalog with nothing local to serve — and `gglib q --remote`
answered anyway. The prompts carried words nothing could have had ready,
`bananaramarama` and `kumquat`, and the replies used them, which rules out a
canned answer as well as a local one.

**The counter read 0, then 1, then 4, and the 1 is the part worth writing
down.** `gglib remote status` on the serving machine reported `Requests:  0
through the tunnel` before the connect, `1` immediately after `connect`
returned and **before any prompt had been sent**, and `4` after the inference
requests. That first increment is the pairing POST itself: `POST
/v1/remote/pair` sits outside the bearer group but inside `remote_marker`,
which runs on every route, so the request that fetches the key counts like any
other tunnelled one. What shows inference crossed is the increment past 1, not
the number being non-zero. Recorded because the next person to read this
counter will otherwise be one ahead of the prompts they remember sending.

> **Amended 2026-09-07 — the quoted line has since changed wording, and the
> quote above is right.** [#986](https://github.com/mmogr/gglib/pull/986)
> landed later the same evening and made it read
> `Requests:  N served through the tunnel`, printed only on the serving
> machine; the third kill criterion above was rewritten with it, which is why
> the two strings differ in one document. `Requests:  0 through the tunnel` is
> what the build this session ran actually printed, and it is left exactly as
> it was read.

**Both transport paths were exercised, so the relay is a path traffic has
taken rather than one the design merely provides for.** On the home network
both sides reported `Path:      direct`: the hole punch held and no relay was
involved. A second pairing was then made with the connecting machine on a
phone hotspot — ticket `dc82a49cf02c`, connect side on port 36073, the
connecting machine appearing under a new fingerprint — and the serving machine
logged `peer{peer=d702c7dca654 path="relayed"}` followed by `POST
/v1/chat/completions status=200 outcome="forwarded"`.

> **Amended 2026-09-07 — weakened, because the instrument cannot carry this
> sentence.** In the `modelpipe` version this ran against — 0.2, the pin —
> `peer::path_of` reads `Connection::paths()` once and is never asked again
> for the life of a connection. The serve side samples it at accept and hands
> the answer to the peer registry, which has no per-peer path mutator; the
> connect side samples it at dial and re-samples only on a re-dial. `path_of`
> also folds "no selected path yet" into `Relayed` deliberately, as the
> conservative reading of an unfinished handshake.
>
> The two halves are therefore not equally strong. `Path: direct` on the home
> network is a positive reading: a selected non-relay path existed at the
> moment it was taken, and nothing else prints `direct`. `path="relayed"` on
> the hotspot was taken at accept, before the punch had had time to succeed or
> fail, and a session that hole-punches a moment later goes on reporting
> `relayed` for the rest of its life. The `status=200` that followed it is
> real; which path carried it is not something this build could see. So the
> line says *the relay was not ruled out*, not *the relay carried it*.
>
> What the session establishes is the direct path, exercised and observed.
> Whether traffic ever crossed a relay is a question the evening could not
> answer with the instrument it had, and the claim that "the relay is a path
> traffic has taken" is withdrawn until one can. The fix is in flight
> upstream — [modelpipe#50](https://github.com/mmogr/modelpipe/pull/50), a
> watcher following `PathEvent::Selected` instead of sampling once. A re-run
> against a version carrying it is what would settle the sentence; nothing in
> this note does.
>
> **Still not readable at the 0.3 pin, 2026-09-07.** modelpipe cut `v0.3.0`
> at `4cf01ee`, and #50 merged after it — the published tarball has neither
> `path_watch.rs` nor `network.rs`, and its `peer::path_of` is the
> sample-once form quoted above, unchanged. So taking 0.3 moves the pin and
> not this sentence. The re-run waits on a modelpipe release cut from a
> commit that contains #50.

**The credential moved as decision 3 describes.** The six-digit code was
redeemed over the encrypted hop, once, and the connecting machine reports
`has_remote_key: true` with the key persisted, so a later `connect` needs the
ticket alone. That is decision 4's trade taken in the direction it was argued
for.

**The kill switch works from the far side.** `gglib remote kill` produced
`remote shutdown requested by an authenticated client; stopping the daemon`
and `POST /v1/proxy/shutdown status=202`, and the daemon stopped. The one-way
door in decision 7 opens.

- **If a dependency ships a first-party remote transport** — **still not
  evaluated.** `PINNED_LLAMA_RELEASE` is still `b10327` and the pin has not
  moved, so the survey has still had no occasion to be taken. **OPEN, and
  unread rather than clean.**
- **If the trust model itself proves unsound** — **still not evaluated.** The
  issue query was not run for this note either, and a session in which the
  feature worked is not evidence about the premise in any case. **OPEN.**
- **If `tunnelled_requests` stays at zero** — **4 through the tunnel in one
  daemon run, 2026-09-06**: one pairing POST and three inference requests, one
  peer, one evening. The criterion has moved from unreadable to readable
  rather than from open to settled. A zero taken after this can be read as a
  zero, where every zero before it was the absence of the test. Four requests
  in one session says the tunnel works; it says nothing about whether anyone
  uses it, which is what the criterion asks. **OPEN.**

**All three remain OPEN**, and only the third moved at all. What the session
settles is that the feature does what the Decision section says it does:
across two operating systems, ~~on both transport paths~~ (struck 2026-09-07:
on the direct path, with the relay neither observed nor ruled out — see the
amendment above), with the key moving
once and the kill switch reachable from outside. What it does not settle is
use — one evening and one peer cannot distinguish a tunnel people want from a
tunnel that merely works. Two gaps are worth naming rather than leaving to be
inferred. The only client on the connecting side was gglib's own CLI, so
decision 7's third-party arrangement — an OpenAI-compatible client pointed at
the connect listener with the key as its API key — is still untried. And a
single evening says nothing about a tunnel left up for days, which is the
shape the rotation poller and the per-session identity were designed against.

### Third reading, 2026-09-07 — the trust-model query, run

Both earlier readings recorded the second criterion as unread, on the grounds
that a zero from a query nobody ran is the failure these criteria exist to
avoid. The query has now been run, and this note exists for that one line.

The other two are **not** re-read here and stay where the second reading left
them: `PINNED_LLAMA_RELEASE` is still `b10327` and the pin has not moved, so
the survey still has had no occasion to be taken; and `tunnelled_requests` has
not been re-read, which by this criterion's own terms is all that can be said —
it counts from daemon start and the daemons that produced the second reading
have stopped, so there is no surface that could report on the interval.

- **If the trust model itself proves unsound** — **0, and the zero is 0 of 0.**
  `gh issue list -R mmogr/gglib --label "priority: critical" --label
  "component: proxy"` returns nothing, and the same query with `component: gui`
  returns nothing. The denominator is the part that matters and it is not 14:
  fourteen issues are open, and **not one of them carries `priority: critical`
  at all**. The label exists — `gh label list` shows it, "Blocking issues,
  security, data loss" — and the bucket is empty repo-wide. So this reading
  cannot distinguish "no critical trust-model issue has been filed" from "this
  repo does not triage with that label", and the two license very different
  conclusions. Answered rather than clean. **OPEN.**

One thing not to fix. The criterion names the missing `component: remote`
label and argues for naming that gap rather than pretending to a label that
does not exist; nothing in this reading overturns that argument. Adding
`component: remote` to the issue form would narrow a query this ADR widened on
purpose — a change of mind about how the criterion is read, not housekeeping.
The reading that would actually make this line clean is a `priority: critical`
label somebody uses.

## Out of scope

Named here so that their absence reads as a decision rather than an oversight.

- **A phone client.** ~~iroh compiles to wasm and would run relay-only, still
  end-to-end encrypted.~~ That is a product, with its own surface and its own
  release story, not a flag on this one.

  > **Amended 2026-09-07 — the technical reason was already out of date when
  > this was accepted; the decision is not.** wasm-and-relay-only was not the
  > state of the art on 2026-09-05.
  > [#963](https://github.com/mmogr/gglib/issues/963) — opened 2026-08-30, six
  > days before this ADR, and the issue this whole feature answers — records it
  > in its own out-of-scope note: iroh 1.0 shipped official Swift/Kotlin
  > bindings, so a native iroh-speaking mobile client is buildable, getting the
  > same direct-or-relay behaviour every other peer gets rather than being
  > relay-only by construction. A phone client stays out of scope for the reason
  > the bullet gives second — it is a product with its own surface and its own
  > release story — which was always the stronger of the two. What is withdrawn
  > is the capability claim in front of it, which was wrong, and which is the
  > kind of sentence that gets read as *we looked and it cannot be done*.
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
