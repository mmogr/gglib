# ADR 0012 — The remote tunnel: one key at two doors, a code that dies on use, and a ticket that dies with the session

- **Status:** Accepted
- **Date:** 2026-09-05 (changed in place before ADRs froze: decision 4 on
  2026-09-09 and reversed 2026-09-10, decision 2 on 2026-09-11)
- **Depends on:** [ADR 0008](0008-two-binaries-one-daemon.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** [log-0012](log-0012.md)

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

> **Amended 2026-09-12 — the call is no longer `set_token`.** Under
> `TokenPolicy::Named` the long-lived object receives `add_token` at each
> invite, `remove_token` at each `forget`, and `set_backend_auth` at each
> rotation of `proxy_api_key` (decision 2's amendment of 2026-09-11). All
> three arrive while the listener is up, so the placement argument holds for
> each of them.

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

> **Amended 2026-09-11 — one credential becomes one credential per device,
> and the second door stops checking tunnelled traffic.** The title of this
> decision is now half true, and saying which half is the point of this
> amendment.
>
> The serve side is constructed with `TokenPolicy::Named` rather than
> `TokenPolicy::Supplied(proxy_api_key)`. Under `Named` the listener starts
> admitting nobody and admits only keys added by name, one per paired device,
> minted when that device is invited. `ServeOptions::backend_auth` carries
> `proxy_api_key`: the edge replaces the device's `Authorization` with it on
> every admitted request, so the proxy still receives the credential it
> demands and no device ever holds it.
>
> Five things follow, and each has to be recorded rather than asserted away.
>
> 1. **The local half of this decision survives untouched.** `enable` still
>    force-generates and persists `proxy_api_key`, and the paragraphs above
>    about the loopback proxy closing, the floor arriving at the next restart,
>    and how to turn it back off are all unchanged. A device key never reaches
>    `bearer_guard`, so nothing a device holds opens the desktop's own proxy.
>    It was tempting to stop minting the key on the grounds that the edge no
>    longer enforces it; that would silently reopen the local proxy, `/mcp`
>    included, which is what this decision exists to prevent.
> 2. **The second door no longer checks tunnelled traffic.** This decision
>    says a bad bearer "would be refused again by the proxy's own
>    `bearer_guard` if it got there. The second check is not redundant." With
>    `backend_auth` set, `bearer_guard` is validating a header modelpipe wrote
>    microseconds earlier, and cannot refuse anything that crossed the edge.
>    That is an unavoidable consequence of per-device keys — there is no shape
>    in which the proxy checks a credential the device holds *and* the device
>    never holds the proxy's — and it is a real loss, not a redundancy being
>    tidied up. For local requests the second door is the only door and is
>    unaffected.
> 3. **What replaced it, and what it does not cover.** A `route_layer` on the
>    protected group refuses any tunnelled request the edge did not name a
>    device for, with 403 `device_not_paired`. The discriminator is
>    `X-Modelpipe-Device`, which the edge writes only when a *named* token
>    admitted — never for a one-time grant, and there is no primary under
>    `Named`. The case that makes it necessary: `Credential::forward` applies
>    `backend_auth` to every admitted request including a **grant**-admitted
>    one, and a grant is one request at any path the holder likes, because the
>    edge cannot scope it. Without the gate, one correctly guessed six-digit
>    code would buy a single fully authenticated request to any protected
>    route — `POST /v1/proxy/shutdown` among them, which is irreversible
>    without physical access. The gate is applied outside `mcp_tunnel_guard`,
>    so a grant-admitted request is refused before `--allow-mcp` is ever
>    consulted. What the gate does *not* address is the open question decision
>    5 now owns: a **named** device reaches `invoke_tool` when `--allow-mcp`
>    is on, and there are now N credentials that can rather than one.
> 4. **`POST /v1/remote/pair` refuses a device that already holds a key.**
>    That route is outside the bearer group by decision 3's design — it cannot
>    demand the credential it exists to hand out — and under `Named` a paired
>    device reaches it with its own valid bearer, which the edge admits for
>    any path. Decision 3 says a wrong code over the tunnel is a wrong bearer
>    the edge refuses before this handler runs; that stops being true for a
>    device that has one. Left open it would let a compromised but
>    not-yet-retired laptop burn every invite a person types, three wrong
>    codes at a time, and take three guesses per invite at a second identity
>    that would survive the first being forgotten. A request the edge named a
>    device for is therefore refused before the code is looked at, with the
>    same flat 401 as every other refusal here.
> 5. **What retiring a device cuts, and where the keys live.** Retiring a
>    device is `ServeHandle::remove_token`, which gates *admission* and not
>    delivery: a response already streaming to that device runs to completion.
>    `gglib remote disable` is the hard stop. The keys are kept in
>    `<data root>/data/remote_devices`, `0600`, beside the endpoint identity,
>    and deliberately not in settings — `gglib config settings show` prints
>    settings unmasked by design and that output is what people paste into bug
>    reports. Hashing at rest was not an option either way: modelpipe compares
>    named tokens in plaintext, so *where* they sit is the only lever there
>    is. Settings keeps ids, labels, when the invite was minted, when a device
>    redeemed it, and when it was last seen — none of which is secret.
>
>    *Amended 2026-09-11.* The redeemed marker was added when the surfaces
>    landed, because without it the roster cannot answer the question a person
>    asks of it. `joined_at` is when the code was **minted** — the row and the
>    key are written before the code is shown, so that a device cannot end up
>    holding a key this side has no record of — so an invite nobody redeemed
>    and a device that paired a minute ago and has not yet made a request read
>    identically. Unspent invites are **listed rather than swept on a timer**
>    (what the machine issued should be visible, and the key was never
>    transmitted so nobody holds it), which makes telling them apart the whole
>    job of the list. Both the marker and `last_seen` are written by background
>    tasks and either can be lost, so a row is called never-joined only when
>    **both** are empty: whichever write went missing, a device that has
>    plainly made requests is never described as one that never arrived.
>
> The cost accepted alongside: a rotation of `proxy_api_key` can 401 tunnelled
> requests for up to two settings-cache windows while `backend_auth` and the
> proxy's own bearer disagree. The proxy builds its own `SettingsCache` and
> the rotation poller sleeps a full TTL between ticks, so the two phases are
> unrelated. `set_token_with_grace` does not help — it widens *admission*, and
> this mismatch is downstream of admission. In exchange, a rotation no longer
> un-pairs every device, which is the wart this change exists to remove.
>
> One thing this does not yet surface, recorded so its absence is a known gap
> rather than an oversight: modelpipe counts a wrong bearer against every live
> bounded grant, so a retired device still issuing requests can burn an open
> invite before it is typed. modelpipe 0.5 exposes no way to ask whether a
> grant was burned, so the serving machine cannot report it. The preconditions
> are narrow and each invite is a fresh grant, so the effect is a retry rather
> than a lockout — but it needs an upstream API before `status` can say so.

> **Amended 2026-09-12 — the poller calls `set_backend_auth`, not
> `set_token`.** Since 2026-09-11 `rotation_poll` hands a changed
> `proxy_api_key` to `ServeHandle::set_backend_auth`, which changes only what
> the edge presents to the proxy. It must never call `set_token`: under
> `Named` that would give the listener a primary key and turn it back into a
> shared-key listener with no error (`remote/rotation.rs`). The skip of a
> cleared or unchanged value, and the pinned case that never spawns the
> poller, are as the 2026-09-07 note describes.

> **Amended 2026-09-12 — under `Named` the edge never asks for the proxy's
> key, so a clear cannot reach it.** The paragraph beginning "The tunnel edge
> does not follow it down", and the 2026-09-07 bullet "A running tunnel does
> not reopen with it", say the edge keeps demanding `proxy_api_key` after a
> clear. Since 2026-09-11 it has never demanded it: the edge admits only
> device keys and live codes, and refuses the proxy's own key. The conclusion
> stands for a different reason. A clear touches no device key, so the edge
> refuses what it refused before, and `rotation_poll` ignores the clear, so
> `backend_auth` goes on presenting the old key to a proxy that no longer asks
> for one. The clear opens the local door only. Item 1 above calls these
> paragraphs unchanged, and this is the one mechanism in them that changed.

### 3. Pairing moves a one-time code, not the key

`gglib remote invite` prints the ticket and a six-digit numeric code, as does
`gglib remote enable --invite`, which does both in one command for a first
run. (Until 2026-09-11 a plain `enable` was the only thing that printed one;
it is a switch now and hands out nothing.) The code lives 120 seconds, is
spent on first use, and is burned after three wrong attempts. Under it, the
desktop calls `ServeHandle::grant_once(code, 120s)`,
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

> **Amended 2026-09-12 — three things above describe the design before
> 2026-09-10 and 2026-09-11.** The response to `POST /v1/remote/pair` carries
> a key minted for that one device when it was invited, not `proxy_api_key`,
> which no device paired since holds (decision 2's amendment of 2026-09-11).
> The grant is `grant_once_bounded`, which still admits exactly one request
> bearing the code, within the two minutes, without a device key;
> `grant_once` under *Out of scope* means this call. And the burn now fires
> through the tunnel: the edge counts wrong bearers and drops the grant at the
> third (decision 4's amendment of 2026-09-10), so the finding above that
> "through the tunnel, the burn never fires" describes builds before that
> date. The ticket still carries an endpoint id nobody can guess, but it
> lasts, so the arithmetic rests on the burn and the window rather than on
> the ticket. The local path is as the 2026-09-07 note says: the proxy's
> counter fires, and the ticket is not needed.

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

> **Amended 2026-09-09 — the alternative is now available, opt in, and the
> default is unchanged.** `gglib remote enable --keep-identity` passes
> modelpipe a stored endpoint key at `<data root>/data/remote_identity`, so
> the ticket survives a restart and a device pairs once instead of every
> session. Without the flag `identity` is still `None` and this decision reads
> exactly as it did.
>
> What changed is not the argument above but who it is being made for. The
> paragraph weighs free revocation against *saved typing*, and typing is what
> it costs a laptop: the person is already at a keyboard with the ticket on
> screen beside them. It costs a **phone** a camera, a QR code on a screen in
> another room, and a walk to that room — every time the desktop reboots,
> which is not a thing people do rarely. Against that, "restarting is
> something people do anyway" stops being the argument for free revocation and
> becomes the argument against the feature being usable.
>
> So the trade is offered rather than made. The default stays where the
> reasoning above puts it, because that reasoning is right for a machine that
> can re-pair cheaply. `--keep-identity` is for the one that cannot, and it
> says what it costs: revocation stops being a reboot and becomes deleting
> `remote_identity` and restarting, which is the same re-pairing of every
> device that used to happen by accident. modelpipe mints the file `0600` and
> refuses to read one others can read.
>

> **Amended 2026-09-10 — this decision is reversed. The identity always
> lasts, and `--keep-identity` is gone.** `enable` writes the endpoint key to
> `<data root>/data/remote_identity` unconditionally, and `remote_enabled`
> makes remote access a switch the daemon honours at startup. There is no
> flag, because there is no longer a choice to offer.
>
> The 2026-09-09 amendment above offered the trade rather than making it, ~~and
> a fortnight of using it settled the question it left open: nobody chose the
> default. Every machine that mattered ran `--keep-identity`, because every
> machine that mattered had a phone or a laptop pointed at it. A default
> nobody keeps is not a default, it is a step.~~
> Retracted 2026-09-26: [log-0012](log-0012.md#2026-09-26-decision-4s-reversal-is-retracted-where-it-cites-a-fortnight-of-use).
>
> But the reversal does not rest on that. It rests on what became true
> underneath it.
>
> **The argument above was never about identities; it was about decision 3's
> arithmetic.** Six digits and two minutes are enough because a guesser must
> find the listener first, and a fresh identity per session is what made
> finding it hard. A lasting ticket removes that step, so the honest reading
> of decision 3 is that it depended on decision 4 and nobody had written that
> down. Under the 2026-09-09 amendment, `--keep-identity` quietly weakened
> the pairing code for anyone who used it — which was, per above, everyone.
>
> What closes it is that the counting moved to where the guesses arrive.
> modelpipe 0.5's `grant_once_bounded` burns a grant at the edge after three
> wrong bearers, so a guesser gets three tries at the tunnel rather than
> unlimited tries at a route the old counter sat behind. gglib's own
> `MAX_ATTEMPTS` never saw those attempts: it counts redemptions reaching the
> proxy, and a wrong bearer never reached it. Decision 3's arithmetic is
> restored by the burn, not by the identity, and it is now restored where it
> is actually enforced.
>
> **A ticket is not a credential.** This is the reframing the original
> paragraph gets wrong by calling it "bearer material". A ticket names a
> machine and says how to reach it; everything behind it takes a key this
> side issued, and per-device keys make that key revocable one device at a
> time. Treating an address as a secret bought a revocation nobody was
> reaching for, and charged for it daily.
>
> **What this costs, kept in plain sight.** A lasting endpoint key is a
> lasting identifier: a machine that publishes to n0's discovery service is
> announcing the same name every day, so anyone watching discovery learns
> when it is up. That is a presence leak, it is real, and it is the price.
> It buys pairing once instead of every reboot. `--no-discovery` avoids the
> leak and costs resolution when the machine changes network, and `enable`
> now says so in two lines rather than one, because with a lasting ticket
> that flag's failure is permanent rather than lasting until the next
> `enable`.
>
> Revoking is deleting `remote_identity`, and `gglib remote status` prints
> the path, because a revocation nobody can find is not one.
>
> **Decision 2 is retained, explicitly.** Device keys never reach the local
> proxy, the local lock stays, and deletion is still deferred. Nothing here
> touches that.
>
> The file sits under `data/` rather than beside `pids/`, and that is
> load-bearing rather than tidy: a debug build resolves the data root to the
> repository checkout, where `.gitignore` covers `/data`. A private key a
> level up would be untracked in a working tree rather than ignored by it.

> **Amended 2026-09-12 — deleting `remote_identity` revokes no key.** Since
> 2026-09-11 a device is admitted by its own key, and ~~every key in
> `data/remote_devices` is put back on the listener whatever identity it
> binds with~~.
> Retracted 2026-09-15: [log-0012](log-0012.md#2026-09-15-what-the-move-supersedes-in-adr-0012-and-the-decisions-taken-with-it).
> Deleting the identity retires the address from the next arm and
> leaves every key admitted, so the revocation this amendment names is
> `gglib remote forget`, one device at a time. A client that learns the new
> ticket and still holds its key is let in; `gglib remote join` asks for a
> fresh code first, and pairing again mints a second key while the first
> stays admitted.

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

> **Amended 2026-09-11 — the leaked token is now one of several.** Under
> per-device keys (decision 2's amendment of the same date) each paired device
> holds a credential of its own, so with `--allow-mcp` on there are N keys
> that reach `invoke_tool` rather than one. The default is unchanged and the
> reasoning above is unchanged; what changes is the arithmetic behind "a
> leaked bearer token". Retiring one device is now a real answer to a leak —
> it used to be a rotation that cut off everybody — but the flag still grants
> tool execution to every device at once, and there is no per-device `/mcp`
> grant. If one is wanted, it is a new decision, not a refinement of this one.
>
> The gate that decision 2's amendment adds does not narrow this either. It
> refuses tunnelled requests the edge did *not* name a device for, which is
> the grant-admitted case; a named device passes it and reaches this guard
> exactly as before.

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

> **Amended 2026-09-12 — the verb moved on 2026-09-10.** `gglib remote kill`
> is now `gglib daemon stop --remote`
> ([ADR 0013](0013-the-target-is-a-value.md), decision 3): stopping the far machine is a
> `daemon` command pointed at another machine, not a pairing command. It is
> the same one-way door.

## Consequences

**Good:**

- Remote access with no account, no VPN profile on the client, and no third
  party that can read a request. The relay sees ciphertext or is not in the
  path at all.
- One credential to reason about. The token that gets a request through the
  tunnel edge is the token the proxy checks, so there is one answer to "what
  is my key" and one place to rotate it.

  > **Amended 2026-09-12 — no longer one credential; see decision 2's
  > amendment of 2026-09-11.** Each device holds a key of its own, which the
  > edge admits by name and replaces with `proxy_api_key` before the request
  > reaches the proxy. The proxy's key still has one answer and one place to
  > rotate it, and no device paired since holds it. A device that paired under
  > the shared key was handed `proxy_api_key` itself and keeps it until it
  > pairs again; the edge no longer admits it, and one rotation of
  > `proxy_api_key` makes that copy worthless at the proxy too. A device is
  > retired on its own, with `gglib remote forget`. What this gives up is the
  > second check on tunnelled traffic: the proxy validates a header the edge
  > wrote and cannot refuse anything that crossed it, and the device gate
  > stands in its place (items 2 and 3 of that amendment).
- A ticket alone is useless, and it expires when the session does. A code
  alone is useless, and it expires in two minutes or on first use.

  > **Amended 2026-09-12 — the ticket no longer expires with the session;
  > decision 4 was reversed on 2026-09-10.** The identity lasts, so a ticket
  > lasts as long as `data/remote_identity` does. Deleting that file retires
  > every ticket the next time the listener binds — a daemon restart, or
  > `disable` then `enable` — because modelpipe reads it only then. It
  > retires no device key: ~~every key in `data/remote_devices` is put back on
  > the new listener~~, so a device is still cut off with `gglib remote forget`.
  > Retracted 2026-09-15: [log-0012](log-0012.md#2026-09-15-what-the-move-supersedes-in-adr-0012-and-the-decisions-taken-with-it).
  > A ticket alone is still useless: under `TokenPolicy::Named` the listener
  > admits only a device's key or a live code.
- The GUI gets a Remote toggle for free, because the logic is daemon-side.

**Costs, accepted:**

- The ticket has to be moved to the laptop every session. This is the direct
  price of decision 4 and the thing most likely to be re-litigated; the
  counter-argument is in modelpipe's ADR 0002 and it is not weak. *Amended
  2026-09-09: it was re-litigated, and `--keep-identity` is the answer — opt
  in, default unchanged. See the note under decision 4.*

  > **Amended 2026-09-12 — this cost is gone, and a different one replaced
  > it.** Decision 4 was reversed on 2026-09-10 and `--keep-identity` removed:
  > every identity lasts, so a ticket is moved once per device rather than
  > once per session. The price is a lasting identifier. A machine that
  > publishes to n0's discovery service announces the same endpoint id every
  > day, so anyone watching discovery learns when it is up; decision 4's
  > amendment of 2026-09-10 accepts that as the cost.
- `--no-discovery` and a per-session identity interact badly by construction:
  a ticket that carries only its minting addresses, from an endpoint that will
  not exist next time, is nearly useless. Both flags are documented; the
  combination is not recommended.

  > **Amended 2026-09-09 — with `--keep-identity` this inverts, and the two
  > flags become opposites rather than a bad pair.** A stored identity is
  > exactly what makes discovery worth having: the endpoint keeps its name
  > across a restart, and discovery is what turns that lasting name into a
  > reachable address after the machine changes network. Turning discovery off
  > *and* keeping the identity gives a ticket that lasts forever and stops
  > resolving the moment the desktop moves — which is the worst of both, and
  > is now the combination not to recommend. `--keep-identity` on its own,
  > with discovery left on, is the one that pays.

  > **Amended 2026-09-12 — every identity lasts now, so the pair above is
  > `--no-discovery` alone.** The 2026-09-09 conclusion stands with the
  > removed flag taken out of it: `--no-discovery` gives a ticket that lasts
  > and stops resolving when the machine changes network.
- Rotation is eventually consistent within 5 s. A revoked key keeps working at
  the tunnel edge for up to one settings-cache window.

  > **Amended 2026-09-12 — neither sentence holds under per-device keys.** A
  > device's key is retired by `gglib remote forget`, which calls
  > `ServeHandle::remove_token` before it touches either store, so the edge
  > refuses that device from the next request onward; a response already
  > streaming to it runs to completion (decision 2's amendment of 2026-09-11,
  > item 5). The proxy's own key is never admitted at the edge under
  > `TokenPolicy::Named`, so a revoked one cannot keep working there. A
  > rotation of it costs the opposite: the poller hands the new key to
  > `set_backend_auth` once per settings-cache window, and tunnelled requests
  > can be refused with 401 for up to two windows while the edge and the proxy
  > disagree — the cost that amendment accepts.

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

  > **Amended 2026-09-12 — the premise has changed twice since this was
  > written.** The ticket lasts (decision 4, reversed 2026-09-10) and the
  > bearer token is one per device (decision 2, amended 2026-09-11), so the
  > premise now reads *a lasting ticket plus a key per device*. The reading
  > is unchanged: the same two queries, and still no `component: remote`
  > label.
- If `RemoteStatus.tunnelled_requests` stays at zero across daemon runs long
  enough that a remote session would have shown up, the tunnel is a feature
  nobody uses and it goes, taking the `modelpipe` dependency and both sides
  with it. `gglib remote status` **on the serving machine** — the one that ran
  `gglib remote enable` — is where it and `last_tunnelled_ms` are read. The
  counter is `RemoteGateway`'s in gglib-app-services; the proxy that receives
  tunnel-marked requests ticks it through
  `RemoteGatewayPort::note_tunnelled_request`. The serving side prints
  `Requests:  N served through the tunnel` while serving is on, and a
  `Last one:` line once one has arrived. The connecting machine prints neither,
  only a pointer to where the number lives, because a zero taken there says
  nothing about the tunnel. Both count from daemon start rather than from
  install, so the denominator is a single daemon run and a zero has to be read
  against how long that run was.

### First reading, 2026-09-06

Readings: [log-0012, 2026-09-06](log-0012.md#first-reading-2026-09-06)

### Second reading, 2026-09-06 — the first two-machine session

Readings: [log-0012, 2026-09-06](log-0012.md#second-reading-2026-09-06--the-first-two-machine-session)

### Third reading, 2026-09-07 — the trust-model query, run

Readings: [log-0012, 2026-09-07](log-0012.md#third-reading-2026-09-07--the-trust-model-query-run)

### Fourth reading, 2026-09-09 — a phone, on cellular, direct

Readings: [log-0012, 2026-09-09](log-0012.md#fourth-reading-2026-09-09--a-phone-on-cellular-direct)

### Fifth reading, 2026-09-12 — the relay, a migration on one connection, and a key revoked

Readings: [log-0012, 2026-09-12](log-0012.md#fifth-reading-2026-09-12--the-relay-a-migration-on-one-connection-and-a-key-revoked)

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

  > **Amended 2026-09-12 — built, as its own product, which is what this
  > bullet said it would have to be.** Readings:
  > [log-0012, 2026-09-12](log-0012.md#amended-2026-09-12--built-as-its-own-product-which-is-what-this-bullet-said-it-would-have-to-be)
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
