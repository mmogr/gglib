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

# On the laptop, within those two minutes:
gglib remote connect <ticket>-<code>
gglib q --remote "What does this error mean?"
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
| `--no-discovery` | Do not publish to or resolve through n0's discovery service. The ticket then carries only the paths it was minted with and stops working if the machine changes network. Advanced. |
| `--no-qr` | Plain text; no alternate screen. |

`gglib remote status` shows both sides: whether the tunnel is up, the
ticket's fingerprint (never the ticket), whether the code is still live,
which peers are connected and by what path, and how many requests came
through. `gglib remote disable` takes the tunnel down; the ticket is dead
from that moment.

The desktop's GUI has the same controls in the **Remote** popover beside the
proxy control, with the ticket and code shown once and cleared when a device
pairs or the code runs out.

### The laptop: `connect`, `disconnect`, `kill`

`gglib remote connect <ticket>-<code>` dials the ticket, redeems the code
through the tunnel for the desktop's API key, stores the key and the ticket,
and binds a loopback port that is now the desktop's proxy. It prints the
port. Later, `gglib remote connect <ticket>` uses the stored key, and
`gglib remote connect` with no argument dials the stored ticket.

| Flag | Effect |
|------|--------|
| `--port N` | Bind this loopback port instead of a free one. |
| `--relay URL` | This side's self-hosted relay. |
| `--no-discovery` | Dial only the paths the ticket carries. |

`gglib remote disconnect` closes the port; the desktop and the stored pairing
are unaffected. `gglib remote kill` is different in kind: it stops the
desktop's daemon through the tunnel — proxy, models, downloads — and then
disconnects. It asks you to type `shutdown` first, because nothing can start
that daemon again from the laptop. `--yes` skips the question for scripts.

## Using it

**gglib's own commands** take `--remote` and attach the stored key
themselves:

```bash
gglib q --remote "Summarise this" < notes.md
gglib chat --remote
```

With `--remote`, a model name is forwarded to the desktop rather than looked
up here, and with no model the desktop's proxy picks the model it serves —
the laptop's default model is not consulted, because the desktop may not
have it. `--remote` and `--port` are exclusive: they name different
machines.

**The GUI's chat** goes to the desktop when the Remote popover's *Use it for
chat* box is checked. The choice is per window and is cleared the moment the
connection goes, so a later turn cannot silently land on a machine you
stopped thinking about.

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

**Pairing moves a one-time code, not the key.** The six-digit code is
granted once at the tunnel edge, lives two minutes, dies on first use, and
is burned by the third wrong attempt — and it is useless without the ticket,
which is the only way to reach the route that accepts it. The key itself
travels once, inside the encrypted tunnel, in exchange for that code. Every
refusal is the same flat refusal; a guesser learns nothing.

**A fresh identity every session.** `enable` mints a new ticket each time and
never writes it to disk. Revocation is `gglib remote disable`: the old ticket
reaches nobody afterwards. A laptop that paired before has to be handed the
new ticket, which is the cost of the property.

**Enabling puts the key on the local proxy too.** The tunnel and the proxy
are one listener, so enabling remote access makes the desktop's own loopback
proxy require the API key from then on — and disabling does not take that
away, because authentication turns on and never off by itself. gglib's own
CLI and GUI read the key from settings and carry on; a hand-configured local
client will start getting `401` and needs the key added once. `enable` says
so every time it runs.

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
| `the remote machine could not be reached` | The desktop is off, offline, or has run `enable` again since (a new ticket). Ask for the new pairing. |
| `the far machine refused the pairing code` | The code expired, was used already, or was burned by wrong attempts. Run `gglib remote enable` on the desktop again. |
| `this machine holds no key for that remote` | You gave a bare ticket but never paired with this desktop. Use the full `<ticket>-<code>` string once. |
| `403 mcp_not_allowed_over_tunnel` | `/mcp` is closed over the tunnel. Re-enable on the desktop with `--allow-mcp` if you mean it. |
| A local client on the desktop starts getting `401` | Enabling put the key on the local proxy. Add the key to that client; it stays on after `disable`. |
| `gglib remote enable` says it is already enabled | One session at a time. `gglib remote disable`, then `enable` for a fresh ticket and code. |

## Not yet

A phone client, pairing over the LAN without a ticket, and pinning the
pairing request to a specific peer are noted in the ADR's
[out of scope](adr/0012-the-remote-tunnel.md#out-of-scope) section.
