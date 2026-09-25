# remote

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-complexity.json)

<!-- module-docs:start -->

The remote tunnel — both sides of [ADR 0012](../../../../docs/adr/0012-the-remote-tunnel.md).

`RemoteOps` puts a `modelpipe` listener in front of the running proxy so a
paired machine reaches `http://127.0.0.1:<port>/v1` here from anywhere, over
an end-to-end encrypted p2p connection. It lives in this crate for the reason
`ProxyOps` does: every `*Ops` type is here, the CLI and the GUI are thin
clients over the daemon API, and nothing above this crate ever sees an iroh
type.

# Module Layout

```text
remote/
  mod.rs            — RemoteOps: the type, the two slots, and status
  serve.rs          — RemoteOps: enable, and turning the tunnel on — this
                      machine as the desktop
  serve_switch.rs   — the switch: the resume at start, disable, and the
                      sentences for a serve side that is busy
  serve_arm.rs      — arming the serve side with the slot reserved: the key,
                      the bind, the install
  resume_wait.rs    — the daemon's own resume saying it is working, and
                      `enable` and `invite` waiting it out
  connect.rs        — RemoteOps: join / disconnect / kill_remote — this
                      machine as the laptop
  backend.rs        — taking the tunnel down when the proxy it fronts
                      stops being the one at that address
  connect_dial.rs   — the span of join with the slot reserved and the
                      lock released: the record and the install
  connect_open.rs   — reaching the far machine for a join: pair or wait,
                      and what each refusal says
  connect_watch.rs  — following one connection until it is over, and the
                      dwell that decides when an idle peer is gone
  slot.rs           — the one-at-a-time slot each side occupies: reserve,
                      install, release, take
  stored_pairing.rs — the record settings keep of the machine this one
                      paired with: reading it, writing it, what a dial that
                      has come up owes it, and what a write that fails after
                      the code is spent has to say
  far_daemon.rs     — the one request made *through* the tunnel: stop the
                      far daemon
  gateway.rs        — RemoteGateway: the port the proxy asks (is /mcp open,
                      a tunnelled request arrived), and the invite a session
                      holds
  pairing.rs        — the open invite: one at a time, read without taking it
  invite_watch.rs   — waiting for an invite to end, and handing it to the
                      gateway to record
  key.rs            — which key the tunnel enforces, and when a minted one is
                      written down
  identity.rs       — where this machine's endpoint key lives, and clearing
                      one that holds no key so the daemon can arm
  teardown.rs       — ending a session: cancel, drain, and only then reset
  rotation.rs       — following a key rotation into the running listener
  types.rs          — what the ops are asked for and what they report
```

# Two keys, two doors

The listener enforces a key **per device** (`TokenPolicy::Named`), and none of
them is the proxy's. A device presents its own; the edge names it in
`X-Modelpipe-Device` on the way through, and `backend_auth` replaces the
`Authorization` header with the proxy's key, so a device key never reaches
the proxy and the proxy's key never reaches a device.

The two doors do different work, and the second is not a repeat of the first.
The edge refuses a key it does not hold, and answers a pairing code itself
without forwarding anything, so every request it forwards names a device. The
second door is for a request that does not: markers forged by a client that
reached the proxy directly, or a credential a later modelpipe admits without
naming one. It is
`gglib-proxy`'s `device_gate`, answering `403 device_not_paired`, and it sits
*inside* `bearer_guard`, and has to:
`backend_auth` means the bearer is a header modelpipe itself wrote, so for
tunnelled traffic the device name is the only thing left that can refuse.

Retiring one device is `forget`; the proxy's own key is unaffected by all of
it, and rotating it re-pairs nobody.

`key.rs` decides the *proxy's* key — the one behind the second door — in
order: what the running proxy
actually demands (a `--api-key` flag is pinned and never appears in settings,
so the stored value would be wrong); the stored `proxy_api_key`; or a fresh
key, minted. The last case is the loopback default — nothing minted a key
because nothing was reachable — and it is the only case that writes anything.

**That write happens after the tunnel is up, not before.** It is the one mark
`enable` leaves on the machine, and it is not undoable in practice: the
loopback proxy demands the key from then on, `disable` deliberately leaves it
(ADR 0012, decision 2), and clearing it again would reopen the local proxy —
`/mcp` included — for whatever adopted it in between. So a `modelpipe::serve`
that fails must not have written it: the operator would be authenticating
with nothing to show for it, and nothing would have said so, because the
error is the CLI's `?` and its notice never runs. Everything that can fail
comes before that write; the tunnel that precedes it is undone by dropping
the handle. `enable` still waits one settings-cache window after writing, so
the local door is locked before a ticket exists — it is `enable`'s *return*
that has to be behind that wait, not the bind, and nothing can reach a tunnel
whose ticket has not left the process.

Rotation has no event to hook. The CLI writes the same SQLite file from
another process, so `RemoteOps` polls `proxy_api_key` on the settings cache's
cadence and calls `ServeHandle::set_backend_auth` when it changes — **never
`set_token`**, which would give the listener a primary key it does not have
and quietly turn the per-device door back into a shared one. A rotation
therefore changes what the edge forwards and nothing about who it admits: no
device is re-paired, and none needs to be. A pinned key is never watched,
because nothing in settings may override it.

# Pairing

`enable --invite` — and `invite`, against a session already up — returns the
ticket and a six-digit code exactly once. A plain `enable` is a switch and
returns no code at all. The code is modelpipe's: `ServeHandle::invite` mints
the device's key and name and holds the key at the edge, `enrolment::offer`
writes both stores and only then arms the code, and the edge answers
`POST /modelpipe/pair` itself — two minutes, one redemption, three wrong codes
per endpoint, every refusal the same refusal. Nothing reaches the proxy.
`invite_watch` waits for the invite to end and hands it to the gateway, which
records a device that redeemed: its label, when, and the endpoint it came
from. One invite at a time is this machine's rule, not modelpipe's.

# Ending a session

`disable` and the proxy watcher end a session the same way, through
`teardown.rs`: cancel what was following the tunnel, drain it, and only then
reset the session. What was admitted before `disable` finishes under the
session it was admitted to, except a pairing: modelpipe withdraws a live
invite as the listener closes, where the drain starts, so a pairing request
whose code the edge has not redeemed by then is refused. A device that
redeemed before it is recorded whichever runs first, because whoever takes
an invite out of the gateway records how it ended: the watcher, a new offer,
or the reset, which does so before it lets the roster's writer go.

Draining first costs nothing on the tunnel in exchange: `shutdown_timeout`
closes admission before it waits, and modelpipe withdraws a live invite as
the listener closes, so the drain gives no code a chance to be redeemed.

It costs something on the *gateway*, which is what the session epoch pays
for. Neither caller holds the `live` lock across the teardown — that would
block `status` for the whole five seconds — so a fresh `enable` can find the
slot empty and open a new invite while the previous session is still
draining. A teardown that then cleared whatever it found would withdraw the
code the operator is holding and revoke an `/mcp` grant that was just asked
for. So `begin_session` numbers each session, `Live` carries the number, and
`reset_session_if` clears nothing once a later session has taken the gateway
over. Arming is what establishes the clean slate the superseded teardown will
now never provide, so it clears the paired flag too.

# The address the tunnel fronts

`enable` reads the proxy's bound address once, and a *bind* address is not a
*dial* address. modelpipe screens what it dials and refuses `0.0.0.0`
outright — it names no host, and on Linux dialling it reaches loopback, which
would be an accidental bypass — while a LAN address has to be permitted
before it will dial at all. `BackendUrl::at` does both halves: it rewrites a
wildcard bind to the loopback literal of the same family, keeping the port,
and carries the permission a deliberate LAN bind needs, that bind naming an
interface loopback would not reach. Link-local and public addresses stay
refused however the value is built, which is the rule and not a gap.

gglib used to do this itself, in a `Backend` struct that mirrored modelpipe's
locality rule in order to predict its verdict and set a separate
`allow_private_backend` flag beside the URL. The permission travels with the
URL now, which is why `serve` is handed the whole `BackendUrl` rather than a
string: a bare `&str` or `String` converts through `BackendUrl::dial`, which
permits no private address, so a LAN-bound proxy would stop arming with no
build error to say why.

# The tunnel goes down with the proxy

That address cannot be corrected afterwards: `modelpipe::serve` holds it for
the listener's whole life, a running listener cannot be re-pointed at a new
port, and re-serving would mint a fresh identity — a new ticket, every paired
machine unpaired. So the tunnel goes down with the proxy it fronts, on a
deliberate `POST /api/proxy/stop` as much as on a crash. Leaving it up would
forward tunnelled requests into a port this daemon no longer owns — and with
them the proxy key the edge presents upstream as `backend_auth`.

Two mechanisms watch for that, because the obvious one has a hole. The
supervisor's exit channel is the fast path, but it is published from inside
the proxy task after the serve future returns, so a task that panicked or
that `stop` aborted on its five-second timeout announces nothing at all. So
the watcher also asks `ProxyOps::status()` on a timer, which reads a finished
join handle rather than a message and therefore sees every case — including
a proxy that went away and came back on a different port, which is running
and is still not this tunnel's backend. The address is compared through the
same rewrite `enable` used, so a wildcard bind matches the loopback literal
it was rewritten to instead of tearing down a healthy tunnel every tick.

`enable` asks the same question once more before it commits, because the
watcher cannot answer it in time: `enable` holds the `live` lock for its
whole body — through up to ten seconds of `wait_online`, and on a first
enable the settings-cache wait after it — so a watcher that saw the proxy
exit in that window is parked on the lock until `enable` returns. Without
that check the caller gets a pairing string the watcher invalidates
milliseconds later, and the event stream reads `remote_enabled` then
`remote_disabled` with nothing anywhere saying why the code never worked. So
it fails closed and reports the exit instead.

That check sits *before* the key is written, which is a trade taken
deliberately: asking it afterwards would keep the answer fresh to the last
instant, at the price of a minted key written for a tunnel that is then
refused — undisclosed state, which is the thing being prevented. Asking it
first costs a first enable's settings wait of staleness, and everything that
window can leave behind is visible: the watcher takes the tunnel down the
moment the lock is free, and the key and its notice both reached the
operator.

# The connect side

`join` binds a loopback port here that is the far machine's proxy
(`modelpipe::connect`). It does **not** inject `Authorization` (ADR 0012,
decision 7): gglib's own commands attach the key from the stored pairing, and
a third-party client supplies it as its API key, the ordinary arrangement. A
listener that injected the key would make every process on this machine an
authenticated client of the other one.

With a `<ticket>-<code>` pairing string the dial is `modelpipe::pair`, which
reaches the far machine, presents the code to its edge and keeps the pipe it
paired over, and the ticket and the key that comes back are stored as one
`RemotePairing`. That binding is the point: a
key is issued *by* the machine whose code was redeemed, so a bare ticket is
dialled only when the stored pairing names **that** machine, by fingerprint.
Admitting it on "some key is stored" is what let a dial to a second machine
succeed holding the first machine's key — connected, `status` reporting a
pairing, every request 401. No argument dials the stored ticket. A task
follows the connection's status and clears it when the pipe closes, so
`status` never shows a port that leads nowhere.

`kill_remote` posts the confirmation word to the far proxy's shutdown route
with the stored key, then disconnects. One-way: nothing here can start that
daemon again.

# What the gateway is for

`RemoteGateway` is always installed, tunnel up or not, because `ProxyOps`
attaches it to every proxy it starts. It holds the invite a session has
open, and the paired flag that says whether the code on offer was taken. It
also holds the `/mcp` grant for tunnelled requests — off unless `enable` was
asked for it — and counts tunnelled requests for the status surface. Its
`Debug` reports state and nothing an invite carries.

# One at a time, without holding the lock

Both sides are "there may be exactly one, and building it is slow": a dial
that waits out an unreachable peer, an `enable` that waits five seconds for
the settings cache and ten for a relay. Both held their slot's mutex for the
whole of it, and `status` reads both — so the command someone runs to find
out what is happening was the one that could not answer while anything was
happening, and `disconnect`, whose whole job is ending a hanging join,
queued behind the join it was cancelling. `tokio::sync::Mutex` is
FIFO-fair; there is no jumping the line.

`slot.rs` is the answer. A caller reserves the slot, drops the lock, does the
slow work, and comes back to install. A teardown that lands in between takes
the slot and cancels the reservation's token, so the slow work stops instead
of finishing for nobody — and the install says so rather than binding a port
behind a command that already reported success.

The one thing that may *not* be done after the lock is given back is arming
the gateway. `enable` installs the tunnel and begins its session under one
hold: dropping the guard wakes whatever `disable` is queued behind it, and on
a multi-thread runtime that `disable` runs alongside the lines that follow —
resetting the session and taking the tunnel down while the arming is still on
its way. An invite held after that reset would read as a live code on a
session that is gone, which is why `offer_pairing` checks the epoch. So a
reservation is never a session: a `disable` finds either a reservation with
nothing armed, or a tunnel with its session. Both writes are
in-memory, which is what lets them share the guard at all — the rule is that
nothing *slow* is held across it.

# When a connection is over

`Closed` is modelpipe's verdict and needs no policy. `Idle` is not one: on
the connect side it means the peer went away and modelpipe is re-dialling,
and it says plainly that it cannot tell a sleeping laptop from a dead one.
Nothing here acted on it at all, so a peer that went away left `remote
status` reporting "Connected" and every request answered 502, indefinitely.

`connect_watch.rs` holds the policy: after thirty seconds of `Idle` the far
machine is announced away, and the connection is **kept**. Thirty because
modelpipe's first retry is well inside it, so a dropped packet or a relay
hiccup is never *announced* — `gglib remote status` still shows the live path,
so a blip reads as `(idle)` there. Long enough to outlast one, short enough
that a read a minute after the lid closed says what is true.

The clock is modelpipe's, read through `ConnectHandle::idle_for` on every
turn rather than kept here. `status_changed` coalesces, so a peer reached and
lost again between two of this side's polls arrives as no status at all;
reading afresh puts the grace on the idleness that is actually running rather
than on one the pipe abandoned. The reading is `None` for a closed pipe
exactly as it is for a reached one, which is why `Closed` is answered before
anything here looks at a clock.

An earlier version of `connect_watch.rs` gave up after ninety seconds and
released the port. That is what turned a closed laptop lid into a dead port,
and a different port the next morning; the paragraph above described that
policy long after it had gone. The socket nudge that sat beside the clock has
gone the same way, for a smaller reason: it is
`ConnectOptions::idle_network_nudge` upstream, on
by default, so from the moment gglib pinned modelpipe 0.7 it was running two
of them.

Deciding is only half of it. `conclude` is the other half: the slot cleared,
the local port dropped — modelpipe is still re-dialling behind it, so leaving
it up means a bound port answering 502 for a machine nobody is waiting for —
and the loss announced, once. Both halves take what they act on through a
closure, and for the same reason: a `ConnectHandle` needs an iroh endpoint
and a peer that answers, so anything reachable only through one is a policy
no test can drive.

<!-- module-docs:end -->
