# Remote access

`gglib remote` puts one machine's models on another. The desktop keeps
running gglib as it always has; the laptop gets a loopback port that *is* the
desktop's proxy, over a connection the two machines make directly to each
other — end-to-end encrypted, with no account, no VPN, and no third party
that can read a request. [ADR 0012](adr/0012-the-remote-tunnel.md) has the
reasoning; this page has the commands.

```bash
# On the desktop (the machine with the models):
gglib remote enable
#   → shows a ticket and a six-digit code, once, for two minutes
gglib model list
#   → the names this machine serves; the laptop needs one of them

# On the laptop, within those two minutes:
gglib remote connect <ticket>-<code>
gglib q --remote -m <a name from that list> "What does this error mean?"
```

That is the whole first pairing. Afterwards the laptop remembers both the
ticket and the key it received, so the next session is
`gglib remote connect` with nothing after it, as long as the desktop has not
run `enable` again since.

## The two sides

Both sides live in the gglib daemon, so both survive the terminal that
started them and both are gone when the daemon stops. Nothing is persisted
across a restart on the desktop side; the laptop keeps only the pairing
described below.

### The desktop: `enable`, `status`, `disable`

`gglib remote enable` starts the proxy if it is not running, puts the tunnel
in front of it, and shows the pairing in the terminal's alternate screen —
a QR code, the ticket, and the code — the way `less` shows a file: leaving
the screen restores the terminal, and nothing is left in the scrollback. The
screen goes away by itself the moment a device pairs or the code expires.
`--no-qr`, or a stdout that is not a terminal, prints the pairing as plain
text instead.

| Flag | Effect |
|------|--------|
| `--allow-mcp` | Let requests arriving through the tunnel reach `/mcp`. Off by default; see [What the other machine can reach](#what-the-other-machine-can-reach). |
| `--relay URL` | Use a self-hosted iroh relay instead of the public ones. |
| `--no-discovery` | Do not publish to or resolve through n0's discovery service. The ticket then carries only the paths it was minted with, and stops resolving for good the moment the machine changes network. Advanced. |
| `--no-qr` | Plain text; no alternate screen. |

### You pair once

`enable` is a switch, not a session. It stays on until you run `disable`,
including across reboots: the daemon brings the tunnel back up at startup with
the same flags you enabled it with, and the machine keeps the same endpoint
key — so its ticket is the same ticket, and a device that paired yesterday
still works today.

The key lives at `<data root>/data/remote_identity`, created `0600` and refused
if anything else can read it. `gglib remote status` prints the path.

**Revoking is deleting that file.** It is deliberate rather than accidental,
which is the change: this used to happen at every reboot, re-pairing every
device as a side effect nobody asked for. Deleting it re-pairs every device,
once, when you mean it.

Two things worth knowing about the trade:

A lasting endpoint key is a lasting name. A machine publishing to n0's
discovery service announces the same name every day, so anyone watching
discovery can tell when it is up. That is a presence leak and it is the price
of pairing once.

`--no-discovery` avoids it and costs more than it used to. Discovery is what
turns an endpoint name back into an address after the machine changes network;
without it the ticket carries only the paths it was minted with, and now that
the ticket lasts, "stops resolving" means for good rather than until the next
`enable`. Paired devices then need a new pairing.

`gglib remote status` shows both sides: whether the tunnel is up, the
ticket's fingerprint (never the ticket), whether the code is still live,
which peers are connected and by what path, and how many requests this machine
has *served* through the tunnel. That last number is counted where the requests
arrive, so it is printed only on the machine that is serving; the connecting
side has nothing to count and is told to read the number over there rather than
shown a zero of its own. `gglib remote disable` takes the tunnel down; the
ticket is dead from that moment.

The desktop's GUI has the same controls in the **Remote** popover beside the
proxy control, with the ticket and code shown once and cleared when a device
pairs or the code runs out.

### The laptop: `connect`, `disconnect`

`gglib remote connect <ticket>-<code>` binds a loopback port that is now the
desktop's proxy, waits up to thirty seconds for the desktop to answer, then
redeems the code through the tunnel for its API key and stores the key and the
ticket. It prints the port. The waiting is in that position on purpose: the
port is bound before anything has reached the far machine, and a code redeemed
down a pipe that reached nobody is spent for nothing. Later,
`gglib remote connect <ticket>` uses the stored key, and `gglib remote connect`
with no argument dials the stored ticket.

**The port stays put.** The first connection binds `8180`; every later one
tries the port the pairing was last reachable on, so a client you pointed
at `http://127.0.0.1:8180/v1` once stays pointed at the desktop. Stable, not
fixed: if something else has taken that port, `connect` binds the next free
one, says so, and remembers *that* one instead. `--port` pins it, and is
remembered the same way.

**The port stays bound while the desktop is away.** A desktop that reboots,
sleeps, or changes network does not end the connection here: the port keeps
answering, with `502 tunnel_unavailable`, and the tunnel keeps dialling — for
as long as `connect` is up, with a backoff, and with a nudge to rebind its
socket every minute in case this laptop changed network while suspended.
After thirty seconds of that, `gglib remote status` and the popover say
**away** and for how long, rather than "connected" over nothing; when the
desktop answers, they say so. Nothing needs typing at either end. Only
`gglib remote disconnect`, or this daemon stopping, ends the connection.

| Flag | Effect |
|------|--------|
| `--port N` | Bind this loopback port, and remember it, instead of the last one used or `8180`. |
| `--relay URL` | This side's self-hosted relay. |
| `--no-discovery` | Dial only the paths the ticket carries. |

`gglib remote disconnect` closes the port; the desktop and the stored pairing
are unaffected. Stopping the *desktop* from the laptop is not a `remote`
command at all: it is `gglib daemon stop --remote`, the same command that
stops the daemon here, pointed at the other machine. It stops that daemon
through the tunnel — proxy, models, downloads — and then disconnects, and it
asks you to type `shutdown` first, because nothing can start that daemon
again from the laptop. `--yes` skips the question for scripts.

## Using it

**gglib's own commands** take `--remote`, one flag that means the same
thing everywhere it is accepted: do this on the machine this one is paired
with. It is declared once, on `gglib` itself, so it goes before the
subcommand or after it:

```bash
gglib q --remote -m qwen3 "Summarise this" < notes.md
gglib chat --remote qwen3
gglib chat --remote            # the same machine, the same model, remembered
```

`q` names the model with `-m`; `chat` names it as the positional and has no
short flag for it. With `--remote` the name is forwarded to the desktop
rather than looked up here, and resolved against the desktop's catalogue
and profiles, not this machine's. Name it the first time; after that
`--remote` remembers the model you last asked that machine for — per
pairing, because it is a name in that machine's catalogue — and a turn that
names none uses it. Before anything is remembered, a turn that names none
is refused here with a sentence that says so, rather than answered
`404 Model '' not found` from the other end. `gglib model list` on the
desktop is the list to choose from. The ID form the positional also accepts
is local-only — `gglib chat 7 --remote` sends `"model": "7"` and comes back
`404 Model '7' not found`.

A `{model}:{profile}` suffix travels with the name and is resolved by the
desktop against **its** profiles, which are the ones that govern how it
samples — `gglib chat qwen3:coding --remote`. A suffix the desktop does not
know comes back as a 404 listing the profiles it has. `--profile` is refused
with `--remote` for the same reason: it names a profile configured on the
laptop, and there is no way for it to reach the machine that would apply it.

**What `--remote` reaches** is the *use* side of the desktop, and the line
is: you can use what is on that machine, and you cannot change what is on
it.

| Command | With `--remote` |
|---|---|
| `chat`, `q` | A turn on the desktop, as above. |
| `serve <model>` | Have the desktop load the model now, so the first turn does not wait. Only the name and a numeric `--ctx-size` travel. |
| `model list` | The desktop's catalogue as its proxy publishes it — the names a turn can ask for, and the context each would be served with. |
| `proxy dashboard` | The desktop proxy's live dashboard, through the tunnel. |
| `proxy cache-clear` | Clear the desktop proxy's prompt cache. |
| `daemon stop` | Stop the desktop's daemon. Asks you to type `shutdown`; `--yes` for scripts. |

Every other command is about this machine — `model pull` and `model remove`
change what is on a machine, and that is done at the machine; `config`
writes settings; `remote` manages the pairing itself — and refuses the flag
with a sentence naming what it does reach, rather than ignoring it.
`proxy stop --remote` is refused with its own sentence, because the far
proxy is what carries the request. `--remote` and `--port` are exclusive:
they name different machines. [ADR 0013](adr/0013-the-target-is-a-value.md)
has the reasoning.

**The GUI's chat** goes to the desktop when the Remote popover's *Use it for
chat* box is checked. The choice is per window and is cleared the moment the
connection goes, so a later turn cannot silently land on a machine you
stopped thinking about. Name the desktop's model in *Model on that machine*
beside the box: the same rule as `--remote` applies for the same reason, so
a turn sent without one is refused here rather than answered `404 Model ''
not found` from the other end. That name is remembered across a
disconnection — the usual reconnection is the same desktop again — but it
is only ever sent while the box is on, and it belongs to the ticket it was
typed against: connect to a *different* desktop and the field is empty
again, because a name in one machine's catalog is not a name in another's.
A desktop that ran `remote disable`/`enable` mints a fresh ticket and counts
as a different one — the old ticket died with the session, so reaching it
takes its new ticket regardless.

*Chat on that machine*, under the model field, opens the chat screen against
the desktop and ticks the box as it goes. It is how a laptop with no models
of its own gets there at all: every other route into that screen starts from
a model served here, so without it the box could be ticked and the model
named with nowhere to type. That chat has no Console tab — the log, the port
and the uptime belong to a process on the desktop — and closing it leaves
both the desktop's server and the tunnel up, unlike closing a local chat,
which stops the server it was talking to.

**Any other OpenAI-compatible client** on the laptop can be pointed at the
port `connect` printed, `http://127.0.0.1:<port>/v1`, with the desktop's API
key as its API key. The port does not add the key for you — that is
deliberate; see [Why the port does not inject the key](#why-the-port-does-not-inject-the-key).
The key is the desktop's `proxy_api_key`, which `gglib config settings show`
prints on the desktop. The per-client recipes in [clients.md](clients.md)
apply unchanged apart from the port and the key.

## How it stays private

**The connection is end-to-end encrypted and the relay cannot read it.** The
tunnel is [modelpipe](https://github.com/mmogr/modelpipe) over iroh: QUIC
with TLS 1.3, keyed to the two machines' identities. When a direct path
cannot be hole-punched, a relay carries the packets — and sees ciphertext,
who is talking to whom, and how much. Never content. `--relay` moves even
that to a server you run.

**One key, two doors.** The tunnel enforces the same bearer token the
desktop's proxy enforces. A request without it is refused at the tunnel edge
before a byte reaches the daemon, and again by the proxy if it somehow got
there. Rotating the key on the desktop (`gglib config settings set
--proxy-api-key`) reaches the running tunnel within a few seconds.

**Rotating the key un-pairs every laptop.** That is the same sentence read
from the other end, and it needs saying on its own because nothing warns you.
The new key reaches the tunnel edge; it reaches no machine that already
paired. A laptop keeps whatever it was handed when it redeemed its code, and
there is no path that updates it — only another redemption writes it. From the
rotation onward its requests are refused at the edge with `invalid or missing
bearer token` — a flat refusal the tunnel writes rather than gglib, naming
nothing, because at that point the tunnel is all that has looked at the
request. That is still what a *third-party* client pointed at the port sees.
gglib's own turns no longer stop there: the daemon reads the refusal's
`invalid_api_key` code and says which machine refused, and what to do about it.
Getting back in means `gglib remote disable` and
`gglib remote enable` on the desktop and a fresh `<ticket>-<code>` on every
laptop that was using the old key. The one case that survives a rotation is a
pairing code still on screen when it lands: that code is re-armed with the new
key and redeems normally.

**The key is not something to type in.** There is no
`gglib config settings set --remote-api-key`, and that is deliberate rather
than missing: the laptop's copy is written only by `gglib remote connect`,
which redeems a code and stores the key *together with the ticket it came
from*. A hand-set key could name a machine the stored ticket does not, which
is exactly the desync — connected, holding the wrong machine's key, every
request refused — that keying the record by ticket fingerprint exists to make
impossible. `scripts/check_settings_surfaces.sh` records the exemption with
that reason. Pair again instead; it is one command on each side.

**Pairing moves a one-time code, not the key.** The six-digit code is
granted once at the tunnel edge, lives two minutes, dies on first use, and
is burned by the third wrong attempt — and it is useless without the ticket,
which is the only way to reach the route that accepts it. The key itself
travels once, inside the encrypted tunnel, in exchange for that code. Every
refusal is the same flat refusal; a guesser learns nothing.

> **Corrected 2026-09-07.** Any one path gets two of those three, not all
> three. Over the tunnel a wrong code is a wrong bearer and is refused at the
> edge before gglib sees it, so the three-attempt burn never counts a guess —
> there, the two-minute window and the ticket are the whole defence. On the
> desktop's own loopback the burn does count, but the ticket is not needed to
> reach the route, so a local process can kill a pairing that is on screen
> with three POSTs. [ADR 0012](adr/0012-the-remote-tunnel.md), decision 3, has
> the arithmetic.

**A fresh identity every session.** `enable` mints a new ticket each time and
never writes it to disk. Revocation is `gglib remote disable`: the old ticket
reaches nobody afterwards. A laptop that paired before has to be handed the
new ticket, which is the cost of the property.

**Enabling puts the key on the local proxy too.** The tunnel and the proxy
are one listener, so enabling remote access makes the desktop's own loopback
proxy require the API key from then on — and disabling does not take that
away. gglib's own CLI and GUI read the key from settings and carry on; a
hand-configured local client will start getting `401` and needs the key added
once. `enable` says so every time it runs.

**Turning it back off on a loopback proxy: unset, then rebind — in that
order.** `enable` says authentication never turns off by itself, and it does
not, but there is a supported way to turn it off by hand. It works on a proxy
that binds loopback, which is the default and what `enable` assumes. A proxy
bound anywhere else is covered two paragraphs down, and this procedure does
not do what it looks like there:

```
gglib config settings unset proxy-api-key
gglib proxy stop        # then start it again on loopback — the default host
```

`gglib config settings set --proxy-api-key ""` is *not* it; a blank would read
as "authentication is on" while accepting `Bearer ` from anyone, so it is
refused with `Proxy API key cannot be blank — clear it instead to disable
authentication`. Emptying the API key box in the desktop app's settings does
the same thing `unset` does.

The order is the part that surprises people, because it is the opposite of the
intuition. What matters is the proxy *rebinding*: the token it demands is
settled when the listener binds, so a listener that is already up has to go
down and come back. Unsetting alone reopens the proxy only while the listener
that `enable` closed is still up — that one bound on loopback before the key
existed, so it has no bind-time token to fall back on. Once a proxy has
*bound* with the key in settings, that key becomes its floor and unsetting no
longer opens anything. So unset first and rebind second. Rebind first and the
unset does nothing, which reads as the command having failed when it has not.
`gglib daemon stop` takes the proxy down with everything else and works the
same way.

**Off loopback the same two commands mint a new key instead of removing one.**
If the proxy you are restarting binds a non-loopback host — `gglib proxy --host
0.0.0.0`, a LAN address, a hostname — the rebind does not reopen it. It takes a
different branch: `resolve_api_key`
(`crates/gglib-runtime/src/proxy/api_key.rs`) finds no `--api-key`, finds
nothing stored because you have just cleared it, sees a host that is not
loopback, and *generates* a key rather than binding open — an endpoint on a
network gets a token whether or not anyone asked for one. It then writes that
key back into `proxy_api_key`, so the setting you cleared is populated again,
with a value you have never seen. The proxy comes back closed, your existing
clients start getting `401`, and the `unset` reads as having done nothing.

Read the new key with `gglib config settings show`, which prints
`proxy_api_key` in full rather than masking it. Then either hand it to the
clients, or set one you choose with `gglib config settings set
--proxy-api-key <key>`.

There is no supported way to run an unauthenticated non-loopback proxy, and
that is the point rather than an accident of this procedure: the only path
that returns no token at all is the loopback one. If the endpoint must be
open, bind it to loopback and put whatever you trust — an SSH tunnel, a
reverse proxy — in front of it.

Back on loopback, two things do not come back with the reopening. A tunnel
that is still running keeps demanding the token it was handed — key rotation
follows a *changed* key, not
a cleared one — so this opens the local door only; `gglib remote disable`
closes the remote one. And `/mcp` *does* come back open, along with `/v1/*`,
because it sits behind the same bearer guard. That is the ordinary posture of
a loopback proxy that never enabled remote access, not a new hole, but it is
worth knowing before you unset on a machine with a shell MCP server
configured. [ADR 0012](adr/0012-the-remote-tunnel.md), decision 2, has the
mechanism.

**The proxy, and only the proxy.** The daemon's management API on
`127.0.0.1:9887` — the door `gglib`'s own commands and the desktop app come
through — is not affected. A daemon bound on loopback, which is the default,
settled on no token when it started and keeps asking for none whatever
`proxy_api_key` says later; a daemon started with `--share-lan` already had
its own token before the tunnel existed. Enabling remote access cannot lock
you out of the tool you enabled it with.

**What the network learns.** By default the desktop publishes its address to
n0's discovery service so the ticket keeps working when it changes network,
and does *not* ask the router to open a port (UPnP/NAT-PMP is off on both
sides). `--no-discovery` removes the discovery contact at the cost of
mobility; `--relay` removes the public relays. What remains — a relay
knowing that two machines talk and how much — is observability, not
readability.

## What the other machine can reach

Everything the desktop's proxy serves — `/v1/models`,
`/v1/chat/completions`, the dashboard, `POST /v1/proxy/shutdown` — with one
exception. `/mcp`, the tool gateway, is refused over the tunnel unless the
desktop ran `enable --allow-mcp`, because a leaked key with a shell MCP
server configured on the desktop is remote code execution. The refusal is
a `403` naming the flag; local clients are unaffected. The proxy tells a
tunnelled request apart by a marker the tunnel edge sets and a peer cannot
remove or forge to its advantage — forging it only denies yourself `/mcp`.

## Why the port does not inject the key

The laptop's port could add `Authorization` to every request passing
through it, and then any client on the laptop would work without
configuration. It does not, on purpose: that would make every process on
the laptop an authenticated client of the desktop, which is a larger grant
than the one you made when you paired. gglib's own commands attach the key
because you asked them to; a third-party client supplies it as its API key,
which is the ordinary OpenAI-compatible arrangement.

## Troubleshooting

| You see | It means |
|---------|----------|
| `the remote machine did not answer within 30 seconds` | The desktop is off, offline, or has run `enable` again since (a new ticket). `connect` binds the local port before it has reached anything, so this is the wait for first contact timing out rather than the dial failing. Ask for the new pairing. |
| `the tunnel closed before the remote machine answered` | The local end went away while the dial was still looking. Nothing was sent through it, so the pairing code is unspent — try `gglib remote connect` again with the same string. |
| `Connected: … — away 3m` in `gglib remote status` | The desktop has not answered for that long. The port here is still bound and still dialling; nothing to do but wait for the desktop, or wake it. |
| `Port 8180 was taken by something else, so this is on … instead` | The port the pairing was last reachable on is in use. The new one is remembered; point any client at it, or free the old port and `--port 8180` to pin it back. |
| `the far machine refused the pairing code` | The code expired, was used already, or was burned by wrong attempts. Run `gglib remote enable` on the desktop again. |
| `this machine holds no key for that remote` | You gave a bare ticket but never paired with this desktop. Use the full `<ticket>-<code>` string once. |
| `the remote machine <fingerprint> refused the stored key` | The key this laptop holds is not that machine's current one — usually because `proxy_api_key` was rotated there since you paired, but also if you dialled a bare ticket for a different machine. Re-enable on the desktop and redeem a fresh `<ticket>-<code>`. |
| `invalid or missing bearer token` | The same refusal, unrendered — what a third-party OpenAI client pointed at the loopback port sees, since gglib is not in that request's path to translate it. |
| `403 mcp_not_allowed_over_tunnel` | `/mcp` is closed over the tunnel. Re-enable on the desktop with `--allow-mcp` if you mean it. |
| A local client on the desktop starts getting `401` | Enabling put the key on the local proxy (`:8080`; the daemon on `:9887` is unaffected). Add the key to that client; it stays on after `disable`. |
| `gglib remote enable` says it is already enabled | One session at a time. `gglib remote disable`, then `enable` for a fresh ticket and code. |

## Not yet

A phone client, pairing over the LAN without a ticket, and pinning the
pairing request to a specific peer are noted in the ADR's
[out of scope](adr/0012-the-remote-tunnel.md#out-of-scope) section.
