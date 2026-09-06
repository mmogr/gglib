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
  serve.rs          — RemoteOps: enable / disable — this machine as the desktop
  connect.rs        — RemoteOps: connect / disconnect / kill_remote — this
                      machine as the laptop
  backend.rs        — which local address the tunnel fronts, and taking the
                      tunnel down when it stops being the proxy's
  connect_dial.rs   — the span of connect with the slot reserved and the
                      lock released: the dial, the pairing, the install
  connect_watch.rs  — following one connection until it is over, and the
                      dwell that decides when an idle peer is gone
  slot.rs           — the one-at-a-time slot each side occupies: reserve,
                      install, release, take
  stored_pairing.rs — the record settings keep of the machine this one
                      paired with: reading it, writing it, what a dial that
                      has come up owes it, and what a write that fails after
                      the code is spent has to say
  pairing_string.rs — `<ticket>[-<code>]`, taken apart
  redeem.rs         — the two requests made *through* the tunnel: redeem a
                      code for the key, and stop the far daemon
  gateway.rs        — RemoteGateway: the port the proxy asks (redeem a code,
                      is /mcp open, a tunnelled request arrived)
  pairing.rs        — the one-time code: begin, redeem once, burn on the third miss
  key.rs            — which key the tunnel enforces, or that one must be minted
  rotation.rs       — following a key rotation into the running listener
  types.rs          — what the ops are asked for and what they report
```

# One key, two doors

The listener enforces the **same** bearer token the proxy enforces
(`TokenPolicy::Supplied`). A wrong token is refused at the tunnel edge before
a byte reaches the daemon, and again by the proxy's own guard if it got there.

`key.rs` decides which token that is, in order: what the running proxy
actually demands (a `--api-key` flag is pinned and never appears in settings,
so the stored value would be wrong); the stored `proxy_api_key`; or a fresh
key, persisted. The last case is the loopback default — nothing minted a key
because nothing was reachable — and `enable` waits one settings-cache window
after writing it so the local door is locked before a ticket exists. That
wait is also the one behaviour change a local client will notice: the loopback
proxy now demands the key too, and disabling the tunnel does not take that
away.

Rotation has no event to hook. The CLI writes the same SQLite file from
another process, so `RemoteOps` polls `proxy_api_key` on the settings cache's
cadence and calls `ServeHandle::set_token` when it changes; a pinned key is
never watched, because nothing in settings may override it.

# Pairing

`enable` returns the ticket and a six-digit code exactly once. The code is
granted at the tunnel edge (`grant_once`) so one request bearing it gets
through without the token; the proxy's pairing route asks `RemoteGateway`
whether it is the code this session minted, and takes the key it stands for.
Two minutes, one redemption, three wrong attempts. Every refusal is the same
refusal.

# The address the tunnel fronts

`enable` reads the proxy's bound address once, and a *bind* address is not a
*dial* address. modelpipe screens what it dials and refuses `0.0.0.0`
outright — it names no host, and on Linux dialling it reaches loopback, which
would be an accidental bypass — while a LAN address needs
`allow_private_backend` before it will dial at all. So `backend.rs`
rewrites a wildcard bind to the loopback literal of the same family, keeping
the port, and sets the flag for a deliberate LAN bind, which names an
interface loopback would not reach. Link-local and public binds stay refused,
which is the rule and not a gap.

# The tunnel goes down with the proxy

That address cannot be corrected afterwards: `modelpipe::serve` holds it for
the listener's whole life, a running listener cannot be re-pointed at a new
port, and re-serving would mint a fresh identity — a new ticket, every paired
machine unpaired. So the tunnel goes down with the proxy it fronts, on a
deliberate `POST /api/proxy/stop` as much as on a crash. Leaving it up would
forward tunnelled requests, and the bearer key modelpipe 0.2.0 does not
strip, into a port this daemon no longer owns.

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
whole body — through a settings sleep and up to ten seconds of `wait_online`
— so a watcher that saw the proxy exit in that window is parked on the lock
until `enable` returns. Without the last check the caller gets a pairing
string that the watcher invalidates milliseconds later, and the event stream
reads `remote_enabled` then `remote_disabled` with nothing anywhere saying
why the code never worked. So it fails closed and reports the exit instead.

# The connect side

`connect` binds a loopback port here that is the far machine's proxy
(`modelpipe::connect`). It does **not** inject `Authorization` (ADR 0012,
decision 7): gglib's own commands attach the key from the stored pairing, and
a third-party client supplies it as its API key, the ordinary arrangement. A
listener that injected the key would make every process on this machine an
authenticated client of the other one.

With a `<ticket>-<code>` pairing string the code is redeemed through the
tunnel — as the bearer, so the edge's one-time grant admits the request, and
in the body, so the far proxy can check it — and the ticket and the key that
comes back are stored as one `RemotePairing`. That binding is the point: a
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
attaches it to every proxy it starts. With nothing armed it rejects every
code. It also holds the `/mcp` grant for tunnelled requests — off unless
`enable` was asked for it — and counts tunnelled requests for the status
surface. Its `Debug` reports state and never a code or a key.

# One at a time, without holding the lock

Both sides are "there may be exactly one, and building it is slow": a dial
that waits out an unreachable peer, an `enable` that waits five seconds for
the settings cache and ten for a relay. Both held their slot's mutex for the
whole of it, and `status` reads both — so the command someone runs to find
out what is happening was the one that could not answer while anything was
happening, and `disconnect`, whose whole job is ending a hanging connect,
queued behind the connect it was cancelling. `tokio::sync::Mutex` is
FIFO-fair; there is no jumping the line.

`slot.rs` is the answer. A caller reserves the slot, drops the lock, does the
slow work, and comes back to install. A teardown that lands in between takes
the slot and cancels the reservation's token, so the slow work stops instead
of finishing for nobody — and the install says so rather than binding a port
behind a command that already reported success.

The one thing that may *not* be done after the lock is given back is arming
the gateway. `enable` installs the tunnel and arms its pairing code under one
hold: dropping the guard wakes whatever `disable` is queued behind it, and on
a multi-thread runtime that `disable` runs alongside the lines that follow —
resetting the session and taking the tunnel down while the arming is still on
its way. A code armed after that reset stays live for `PAIRING_TTL` on a
session that is gone, and `POST /v1/remote/pair` is outside the proxy's
bearer group. So a reservation is never a session: a `disable` finds either a
reservation with nothing armed, or a tunnel with its code. Both writes are
in-memory, which is what lets them share the guard at all — the rule is that
nothing *slow* is held across it.

# When a connection is over

`Closed` is modelpipe's verdict and needs no policy. `Idle` is not one: on
the connect side it means the peer went away and modelpipe is re-dialling,
and it says plainly that it cannot tell a sleeping laptop from a dead one.
Nothing here acted on it at all, so a peer that went away left `remote
status` reporting "Connected" and every request answered 502, indefinitely.

`connect_watch.rs` holds the policy: a clock starts on the first `Idle` — the
first, not each, or a re-dial that keeps finding nobody would keep buying
time — and ninety seconds later the connection is taken down and announced.
Ninety because modelpipe's re-dial backoff tops out at thirty and a dial at a
machine that is off takes iroh about thirty more, so one full cycle is around
a minute.

Deciding is only half of it. `conclude` is the other half: the slot cleared,
the local port dropped — modelpipe is still re-dialling behind it, so leaving
it up means a bound port answering 502 for a machine nobody is waiting for —
and the loss announced, once. Both halves take what they act on through a
closure, and for the same reason: a `ConnectHandle` needs an iroh endpoint
and a peer that answers, so anything reachable only through one is a policy
no test can drive.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`backend.rs`](backend.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-backend-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-backend-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-backend-coverage.json) |
| [`backend_tests.rs`](backend_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-backend_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-backend_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-backend_tests-coverage.json) |
| [`connect.rs`](connect.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect-coverage.json) |
| [`connect_dial.rs`](connect_dial.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_dial-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_dial-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_dial-coverage.json) |
| [`connect_race_tests.rs`](connect_race_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_race_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_race_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_race_tests-coverage.json) |
| [`connect_teardown_tests.rs`](connect_teardown_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_teardown_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_teardown_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_teardown_tests-coverage.json) |
| [`connect_tests.rs`](connect_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_tests-coverage.json) |
| [`connect_watch.rs`](connect_watch.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_watch-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_watch-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_watch-coverage.json) |
| [`connect_watch_tests.rs`](connect_watch_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_watch_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_watch_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-connect_watch_tests-coverage.json) |
| [`gateway.rs`](gateway.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway-coverage.json) |
| [`gateway_tests.rs`](gateway_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-gateway_tests-coverage.json) |
| [`key.rs`](key.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-key-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-key-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-key-coverage.json) |
| [`lifecycle_tests.rs`](lifecycle_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-lifecycle_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-lifecycle_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-lifecycle_tests-coverage.json) |
| [`pairing.rs`](pairing.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing-coverage.json) |
| [`pairing_string.rs`](pairing_string.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing_string-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing_string-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing_string-coverage.json) |
| [`pairing_tests.rs`](pairing_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-pairing_tests-coverage.json) |
| [`redeem.rs`](redeem.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-redeem-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-redeem-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-redeem-coverage.json) |
| [`rotation.rs`](rotation.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-rotation-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-rotation-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-rotation-coverage.json) |
| [`serve.rs`](serve.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-serve-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-serve-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-serve-coverage.json) |
| [`slot.rs`](slot.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-slot-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-slot-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-slot-coverage.json) |
| [`slot_tests.rs`](slot_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-slot_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-slot_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-slot_tests-coverage.json) |
| [`stored_pairing.rs`](stored_pairing.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-stored_pairing-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-stored_pairing-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-stored_pairing-coverage.json) |
| [`stored_pairing_tests.rs`](stored_pairing_tests.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-stored_pairing_tests-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-stored_pairing_tests-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-stored_pairing_tests-coverage.json) |
| [`types.rs`](types.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-types-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-types-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-app-services-remote-types-coverage.json) |
<!-- module-table:end -->

</details>
