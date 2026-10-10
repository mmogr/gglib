# Remote access

`gglib remote` puts one machine's models on another. The desktop keeps
running gglib as it always has; the laptop gets a loopback port that *is* the
desktop's proxy, over a connection the two machines make directly to each
other — end-to-end encrypted, with no account, no VPN, and no third party
that can read a request. [ADR 0012](adr/0012-the-remote-tunnel.md) has the
reasoning; this page has the commands.

```bash
# On the desktop (the machine with the models):
gglib remote enable --invite
#   → shows a ticket and a six-digit code, once, for two minutes

# On the laptop, within those two minutes:
gglib remote join <ticket>-<code>
gglib model list --remote
#   → the desktop's models, each by its id there
gglib q --remote -m <an id from that list> "What does this error mean?"
```

That is the whole first pairing. Afterwards the laptop remembers both the
ticket and the key it received, so the next session is
`gglib remote join` with nothing after it — and it stays that way across
restarts on both machines, because the desktop keeps its endpoint key.

The laptop stores one pairing. Joining a second desktop with its
`<ticket>-<code>` replaces the first, and `join` names the machine it
replaced; reaching the first desktop again takes a fresh
`gglib remote invite` there.

Every surface on the laptop shows the desktop by its name: the first label of
the desktop's host name (`desk` for `desk.local`), which the desktop gives on
its `/v1/models` and the laptop reads each time it connects, so a desktop
renamed to another plain label is shown by its new name from the next `join`.
The ticket's fingerprint stays the desktop's identity, which is what is
compared, and is not shown. A desktop that gives no name, or gives a host name
that is not a plain label, keeps the last name it gave; one that has never
given one is shown as "the paired machine". Pairing tells the desktop the
laptop's own name the same way, as the name `gglib remote list` shows the
device by there.

To pair a second device later, `gglib remote invite` on the desktop: it
offers another code against the tunnel that is already up, so nobody else
loses their connection. `gglib remote list` shows what is paired and
`gglib remote forget <id>` retires one of them.

`--invite` is there because `enable` on its own is a switch: it turns this
machine on and hands nothing out. Every device gets a key of its own, minted
when you invite it, so pairing is a separate act from being reachable — see
[A key per device](#a-key-per-device).

## The two sides

Both sides live in the gglib daemon, so both survive the terminal that
started them. They differ in what comes back afterwards: the desktop side is
a switch and a restart puts it back, with the same endpoint key, the same
ticket and the same paired devices; the connecting side is not, and the
laptop dials again from the pairing it stored.

### The desktop: `enable`, `invite`, `list`, `forget`, `status`, `disable`

`gglib remote enable` starts the proxy if it is not running and puts the
tunnel in front of it. On its own it hands out nothing: it prints the ticket
and says the machine is reachable, and that is all. Adding `--invite` also
mints a key for one new device and a six-digit code that hands it over once,
shown in the terminal's alternate screen — a QR code, the ticket, the code,
and the id of the device it is for — the way `less` shows a file: leaving
the screen restores the terminal, and nothing is left in the scrollback. The
screen goes away by itself the moment a device pairs, the code expires, or
the invite is withdrawn on this machine. `--no-qr`, or a stdout that is not
a terminal, prints the pairing as plain text instead.

A restart never invites. The daemon brings the tunnel back up with the flags
you enabled it with, minus this one: a code nobody is watching for is a live
code nobody spends, on a ticket that no longer changes between sessions.

`gglib remote invite` is the same offer without the switch, for every device
after the first. It needs the tunnel up, and waits for one the daemon is
putting back after a start. It leaves it exactly as it was — the flags, the
ticket, and every device already using it — so pairing a second machine
costs nobody else their connection. It takes `--no-qr` and
nothing else; to change the flags, `disable` and `enable` again.

`gglib remote list` shows what this machine has issued a key to, and
`gglib remote forget <id>` retires one of them. A row that reads
**"invited …, never joined"** is a code that was offered and never redeemed:
the key was minted and never transmitted, so in the ordinary case nobody
holds it and forgetting it is tidying rather than revocation.

Ordinary, not certain. The marker that says a device redeemed is written by a
background task and can be lost — to a crash between the redemption and the
write, and to any row a build older than this one wrote for a device that
paired and never made a request. Such a row reads as never joined when it was
not. The list errs this way on purpose, because the other direction would
call an unspent invite a device; but it means `forget` on a row you do not
recognise is a revocation you should be willing to make, not merely tidying.

A device that redeemed its code also says which endpoint it paired from:
the fingerprint of the endpoint the code was redeemed from. The device's key
is pinned to that endpoint: presented from any other machine it is refused as
a wrong key is. A device paired before keys were pinned, or one that has lost
its endpoint key, is listed as not admitted and has to be paired again, after
`gglib remote forget` here if it pairs from the same endpoint. gglib's own
`join` keeps an endpoint key for each machine it joins (see
[The laptop](#the-laptop-join-disconnect-key)), and ggchat keeps one for each
machine it pairs with from 0.3.1, so a laptop or a phone presents the
fingerprint it paired from each time it connects here, for as long as it keeps
that key. A laptop that paired from a gglib that kept no such key has to be
paired again. A row whose redemption was not recorded shows none.

```console
$ gglib remote list
  ID            DEVICE
  dev-4e5f6a7b  Matt's MacBook  (last seen 4m ago · paired from 3ca82708b995)
  dev-0a1b2c3d  Matt's iPhone  (no requests yet · paired from 91d0e4f2a6c8)
  dev-9c2b77f1  —  (invited 3d ago, never joined)

  Retire one:  gglib remote forget <id>
```

| Flag | Effect |
|------|--------|
| `--allow-mcp` | Let requests arriving through the tunnel reach `/mcp`. Off by default; see [What the other machine can reach](#what-the-other-machine-can-reach). |
| `--relay URL` | Use a self-hosted iroh relay instead of the public ones. |
| `--no-discovery` | Do not publish to or resolve through n0's discovery service. The ticket then carries only the paths it was minted with, and stops resolving for good the moment the machine changes network. Advanced. |
| `--no-qr` | Plain text; no alternate screen. |
| `--invite` | Also mint a key for one new device and a code that hands it over, so a first run is one command. Afterwards, `gglib remote invite`. |

### You pair once

`enable` is a switch, not a session. It stays on until you run `disable`,
including across reboots: the daemon brings the tunnel back up at startup with
the same flags you enabled it with, and the machine keeps the same endpoint
key — so its ticket is the same ticket, and a device that paired yesterday
still works today.

Putting the tunnel back takes a daemon a few seconds after it starts, and a
command that arrives meanwhile waits for it rather than being refused.
`gglib remote enable --invite` or `gglib remote invite` typed then gets a code
on the session that came back, and a plain `gglib remote enable` is answered
by that session. Flags passed with a command that waited do not apply to it:
the session keeps the ones it was enabled with, and `disable` then `enable`
changes them. If the tunnel is still not back after twenty seconds, the
command says so and asks you to wait.

The tunnel also comes back after a restart of the proxy it fronts. Stopping
the proxy takes the tunnel down with it, because a running tunnel cannot be
moved to a new listener. Start the proxy again and the daemon puts the
tunnel back — it looks every five seconds — with the same flags, on the same
ticket, and with no code: a device that was paired needs no pairing again.
A command does not wait for this the way it waits at startup: `gglib remote
enable` typed in the seconds it takes is refused with `remote access is
already being enabled`, and `gglib remote invite` with `remote access is
still coming up`; run it again once `gglib remote status` shows the ticket.
Putting the tunnel back never starts the proxy, whether you stopped it or it
crashed, so the tunnel comes back once the proxy is started again and not
before. Stopping the proxy is therefore not how to switch remote access off:
`gglib remote disable` is, and a proxy that comes back after a `disable`
brings no tunnel back with it. The daemon also leaves the tunnel down, with
the switch on, when arming fails with the proxy running, and when the proxy
comes back demanding no key with none stored — which is what turning local
authentication off leaves behind (see
[How it stays private](#how-it-stays-private)). Its log says why, and the
daemon does not try again while it runs: `gglib remote enable` does, and
`gglib remote disable` switches remote access off.

The key lives at `<data root>/data/remote_identity`, created `0600` and refused
if anything else can read it. `gglib remote status` prints the path.

**Deleting that file retires the address, not the keys.** It is deliberate
rather than accidental, which is the change: this used to happen at every
reboot, re-pairing every device as a side effect nobody asked for. It takes
effect the next time the tunnel comes up — a restart of the daemon or of the
proxy, or `disable` then `enable` — and every device then needs the new ticket
to find the machine.
It revokes nothing: the device keys the roster lists are put back on the new
tunnel, so the old key still opens it for any client that presents one with
the new ticket. `gglib remote join` asks for a fresh code instead, and pairing again
mints a second key while the first stays admitted. To cut a device off, use
`gglib remote forget <id>`.

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

`gglib remote status` shows both sides: whether the tunnel is up, the ticket's
fingerprint (never the ticket), the machine this one has joined by its name,
whether the code is still live, which peers are connected and by what path,
and how many requests this machine has *served* through the tunnel. That last
number is counted where the requests arrive, so it is printed only on the
machine that is serving; the connecting side has nothing to count and is told
to read the number over there rather than shown a zero of its own.
`gglib remote disable` takes the tunnel down; nothing answers the ticket until
the next `enable`, which brings the same one back. With no daemon running,
`disable` switches remote access off in settings instead, so the next start
does not put the tunnel back, and says so.

The desktop's GUI has the same controls in the **Remote** popover beside the
proxy control, with the ticket and code shown once and cleared when a device
pairs or the code runs out.

### A key per device

Every device that pairs gets a key of its own, minted on the desktop at the
moment you invite it and never shared with another device. The tunnel edge
holds them by name and admits nothing else.

That is what makes losing a laptop survivable. Under one shared key the only
revocation was rotating it, which cut off every device at once and made
re-pairing all of them the price of retiring one. Now retiring a device stops
that device and nothing else, and **rotating `proxy_api_key` un-pairs
nobody** — a rotation changes only what the tunnel presents to the proxy on
the far side of the edge, which no device ever sees.

The desktop keeps two records, in two places, on purpose:

* **The keys** are in `<data root>/data/remote_devices`, beside the endpoint
  key and with the same `0600` posture. Not in settings: `gglib config
  settings show` prints settings unmasked by design, so that a rotated key
  can be recovered, and that output is what people paste into bug reports.
* **Everything readable about a device** — the id the edge knows it as, what
  it called itself, when it joined, when it was last seen, and which endpoint
  redeemed its invite — is in settings, where nothing about it is secret.

**Retiring a device stops admission, not delivery.** The edge refuses that
device's key from the next request onward; a response already streaming to it
runs to completion. If what you need is for a machine to stop answering *now*,
that is `gglib remote disable`.

**A device id is not a secret and is not private.** It travels as a header on
every request the device makes and lands in logs at both ends. The name a
device calls itself is a label for you to read, and nothing is granted on the
strength of it.

**Pairing is for machines that have not paired.** A device that already holds
a key is refused by the pairing route before its code is read, so one that
has been compromised cannot burn the invite you are typing, or trade it for a
second identity that would outlive the first being retired.

**Pairing a second device never disturbs the first.** `gglib remote invite`
— and `enable --invite` against a tunnel that is already up, which does the
same thing — offers a code against the live session rather than refusing. It
leaves the flags alone —
changing `--allow-mcp` or `--relay` still means `disable` and `enable` again,
and the ticket is the same one afterwards.

**Upgrading breaks existing pairings, once.** A machine that paired before
per-device keys holds the old shared key, and the edge no longer admits it.
Invite each device once and they are back; nothing else about the machine
changes, and the ticket is the same ticket.

### The laptop: `join`, `disconnect`, `key`

`gglib remote connect`, this command's old name, is gone, and so is
`POST /api/remote/connect`. A pairing you already have is unaffected.

`gglib remote join <ticket>-<code>` binds a loopback port that is now the
desktop's proxy, waits up to twenty-five seconds for the desktop to answer,
then presents the code to the desktop's tunnel edge for its API key and stores
the key and the ticket. It prints the port. The waiting is in that position
on purpose: the port is bound before anything has reached the far machine,
and a code presented down a pipe that reached nobody is spent for nothing.
Later, `gglib remote join <ticket>` uses the stored key, and
`gglib remote join` with no argument dials the stored ticket.

**The port stays put.** The first connection binds `8180`; every later one
tries the port the pairing was last reachable on, so a client you pointed
at `http://127.0.0.1:8180/v1` once stays pointed at the desktop. Stable, not
fixed: if something else has taken that port, `join` binds the next free
one, says so, and remembers *that* one instead. `--port` pins it, and is
remembered the same way.

**The port stays bound while the desktop is away.** A desktop that reboots,
sleeps, or changes network does not end the connection here: the port keeps
answering, with `502 tunnel_unavailable`, and the tunnel keeps dialling — for
as long as the connection is up, with a backoff, and with a nudge to rebind
its socket — a minute apart at the closest, and further apart when a dial
takes longer — in case this laptop changed network while suspended.
After thirty seconds of that, `gglib remote status` and the popover say
**away** and for how long, rather than "connected" over nothing; when the
desktop answers, they say so. Nothing needs typing at either end. Only
`gglib remote disconnect`, or this daemon stopping, ends the connection.

**The laptop keeps an endpoint key for each desktop it joins**, so the desktop
sees the same endpoint each time this laptop joins it. The key is at
`<data root>/data/remote_join/<fingerprint>`, named by the fingerprint of the
desktop's ticket, so a desktop that moves to another address keeps its key
here. It is a different file from `data/remote_identity`, the key this machine
*serves* with, because a machine can serve and join at once. Before every
dial, `join` makes the directory `0700` if it is not there, and takes group
and other access away from it if it is; modelpipe mints the key `0600` the
first time a dial needs it, and every later dial, after a `disconnect` or a
daemon restart, reads it back. Windows has no modes to set, so there both
take the permissions of the directory they are made in. A key that cannot be
used — not a key, readable by others, or not a regular file — refuses the
join, and is left as it is. A first key that cannot be written, on a full
disk for example, refuses the join too, and so does a directory that cannot
be made; the message says what stopped it (see
[Troubleshooting](#troubleshooting)). Deleting a key makes the next `join`
mint another, which the desktop sees as a new endpoint.

A lasting key is a lasting name here too. The relay a join goes through, and
n0's discovery service while discovery is on, see this laptop under the same
name each time it joins that desktop, as they see the desktop.
`--no-discovery` keeps it out of discovery.

| Flag | Effect |
|------|--------|
| `--port N` | Bind this loopback port, and remember it, instead of the last one used or `8180`. |
| `--relay URL` | This side's self-hosted relay. |
| `--no-discovery` | Dial only the paths the ticket carries. |

`gglib remote disconnect` closes the port; the desktop and the stored pairing
are unaffected. Typed while a `join` with a code is still pairing, it answers
at once, but the pairing runs on for up to fifty-five seconds, because
stopping it part way could spend the code for nothing; if it finishes, `join`
says this machine holds the key, and `gglib remote join` with no argument
connects. Stopping the *desktop* from the laptop is not a `remote`
command at all: it is `gglib daemon stop --remote`, the same command that
stops the daemon here, pointed at the other machine. It stops that daemon
through the tunnel — proxy, models, downloads — and then disconnects, and it
asks you to type `shutdown` first, because nothing can start that daemon
again from the laptop. `--yes` skips the question for scripts.

`gglib remote key --show` prints this laptop's device key on stdout, and
nothing else, for a client that is not gglib (see [Using it](#using-it)). The
warning that it is a secret goes to stderr. Without `--show` it prints nothing
on stdout and says how to ask; with no pairing stored it exits 1.

## Using it

**gglib's own commands** take `--remote`, one flag that means the same
thing everywhere it is accepted: do this on the machine this one is paired
with. It is declared once, on `gglib` itself, so it goes before the
subcommand or after it:

```bash
gglib q --remote -m 7 "Summarise this" < notes.md
gglib chat --remote qwen3
gglib chat --remote            # the same machine, the same model, remembered
```

`q` names the model with `-m`; `chat` names it as the positional and has no
short flag for it. With `--remote` the model is looked up on the desktop,
not here: by its id there — `gglib model list --remote` on the laptop is the
list to choose from — or by its name, against the desktop's catalogue and
profiles. The desktop answers once, before the turn starts, with the model it
means. The banner names it as `qwen3 (7) on desk`, and the turn sends its id,
`"model": "7"`, so another model of the same name there cannot answer it. A
model the desktop does not have is refused then, before any turn. Name it the
first time; after that `--remote` remembers the model you last asked that
machine for — per pairing, by its id there — and a turn that names none uses
it. Before anything is remembered, a turn that names none is refused here
with a sentence that says so, rather than answered `404 Model '' not found`
from the other end.

A bare id always means this machine, and is never sent anywhere else. While
the laptop is paired, an id or name it does not hold says where to look
instead: `gglib chat 7` with no model 7 on the laptop answers `no model 7 here;
desk's models need --remote (gglib model list --remote)`. `serve` and `model
inspect` say the same.

A `{model}:{profile}` suffix travels with the name and is resolved by the
desktop against **its** profiles, which are the ones that govern how it
samples — `gglib chat qwen3:coding --remote` sends `"7:coding"`. A suffix the
desktop does not know comes back as a 404 listing the profiles it has.
`--profile` is refused with `--remote` for the same reason: it names a
profile configured on the laptop, and there is no way for it to reach the
machine that would apply it.

**An image goes with a turn.** `gglib q --remote --image shot.png "What is
the error?"` and `gglib chat --remote --image diagram.png` attach a PNG or a
JPEG as they do without the flag
([Images](../crates/gglib-cli/README.md#images)). The file is stored on the
laptop, under the hash of its bytes, and the laptop's message names it by
that id. The bytes leave only in the request to the desktop's model, as an
`image_url` part, the form any OpenAI client sends. The whole chat is sent
again each turn, so its images cross the tunnel again each turn: at most
16 MiB of image bytes a request, refused on the laptop before anything is
sent. The laptop does not ask whether the desktop's model reads images. The
desktop's proxy does, and refuses a model with no projector before it loads
it, `400 model_cannot_read_images`; the laptop shows that sentence, with the
command that links a projector, to be run on the desktop.

**A chat resumes on the machine it ran on.** Each conversation stores its
model by machine and by id there, and `gglib chat --continue <id>` goes back
to it: a chat that ran on the desktop resumes on the desktop without
`--remote`, and says so, and refuses `--port`, which names a server on the
laptop; one that ran on the laptop refuses `--remote`, an older one that
stores only its model's id there included. After joining a
different desktop, a chat that ran on the first one is refused, rather than
sent to whatever has its id on the new one. A model named on the command line
follows the flag, as does a conversation that stored no model, and the
conversation then stores that model; a chat whose model has left the laptop's
library says so, and continues once another is named. `gglib chat history`
shows each as `qwen3 (7) on desk` or `qwen3 (3) on this machine`.

**`gglib model list`** without `--remote` ends, while the laptop is paired,
with one line on the desktop: `Paired with desk (direct). Its models: gglib
model list --remote`, `desk: away 2m`, or `desk: not connected`. It is said
from what the laptop already knows, the stored pairing and its daemon's
connection; nothing is asked of the desktop, so the list never waits on the
tunnel. It prints on an empty library too, which is a laptop's usual case.

**What `--remote` reaches** is the *use* side of the desktop, and the line
is: you can use what is on that machine, and you cannot change what is on
it.

| Command | With `--remote` |
|---|---|
| `chat`, `q` | A turn on the desktop, as above: the model resolved there once, then sent by its id. |
| `serve <model>` | Have the desktop load the model now, so the first turn does not wait. Only the model's id or name and a numeric `--ctx-size` travel. |
| `model list` | The desktop's catalogue as its proxy publishes it, one row per model: its id on the desktop, its name, the context it would be served with, and the profiles it can be asked for with. A turn names one as `<id>` or `<id>:<profile>`. |
| `model inspect <model>` | Everything the desktop stores about one of its models, as its own inspector shows it, but for the paths of its file and its projector on its disk; then the command that chats with it. `--json` prints the same detail. |
| `proxy dashboard` | The desktop proxy's live dashboard, through the tunnel. |
| `proxy cache-clear` | Clear the desktop proxy's prompt cache. |
| `daemon stop` | Stop the desktop's daemon. Asks you to type `shutdown`; `--yes` for scripts. |

Every other command is about this machine — `model pull` and `model remove`
change what is on a machine, and that is done at the machine; `config`
writes settings; `remote` manages the pairing itself — and refuses the flag
with a sentence naming what it does reach, rather than ignoring it.
`proxy stop --remote` is refused with its own sentence, because the far
proxy is what carries the request; so are `web --remote` and `gui --remote`,
because the page they open already lists the desktop's models beside this
machine's. `--remote` and `--port` are exclusive:
they name different machines. [ADR 0013](adr/0013-the-target-is-a-value.md)
has the reasoning.

**The GUI's library** lists the desktop's models after this machine's, in a
group headed by the desktop's name and whether it is reached right now. Each
row carries a badge with that name, so a model both machines have is two rows
that cannot be confused, and the library's search matches them as it matches
this machine's. The filters are this machine's library's (its tags, sizes and
quantisations, applied by this daemon), so while one is on the desktop's
group is set aside with a note saying so. The rows are read through this machine's daemon,
which adds the key, while the desktop is connected and answering: again when
it comes back, on each new connection, and when the window regains focus.
While it is away, or a read fails, the last rows stay, marked *away* or
*stale*, as they are while a new connection has not yet said which machine
answered; a disconnection clears them. A read that is slow or fails never
holds up or empties this machine's rows.

Picking a desktop row opens its inspector instead of this machine's: what the
desktop stores about the model, without the paths of its file and its
projector on its disk, and only what `--remote` allows on a model there —
*Chat* and *Load*. Nothing that changes it (edit, tags, delete, Start or Stop here) is offered, and the
native menu's Start, Stop and Remove have nothing selected while a desktop
row is, so they cannot act on a model here that shares its number. A model
the desktop is serving says *Serving on desk*, in place of *Load*, which has
the desktop load it now so the first turn does not wait, and reads it again
when it lands. While the rows are away or stale, both buttons are disabled.
The inspector also shows the command that does the same from a terminal here,
`gglib chat <id> --remote`, since that id without the flag is this machine's.

*Chat* opens a chat with the desktop's model: the conversation is this
machine's, the agent loop runs here, and each turn is sent to the desktop by
the model's id there, so no other model of its name answers. The head reads
`qwen3 on desk`; there is no Console tab (the log, the port and the uptime
belong to a process on the desktop) and no model picker, because a chat's
machine is fixed. The conversation is made for that model, and every run
keeps it on the conversation by its machine, so a paired device's turn on it
is refused rather than answered by a model here, and `gglib chat --continue`
goes back to the desktop. The list beside it holds the desktop's chats and
those that have not run yet, and a chat with a model here lists only this
machine's: a run on another machine than its chat's is refused, `409
conflict`. Pair with a different desktop and a chat, a pick or
a row held for the first one is dropped, because its ids name other models
on the second. Closing the chat leaves the desktop's model and the tunnel
up, as closing a local chat leaves its model loaded.

**The desktop's own chats** are on the chat page too, once joined: *desk's
chats*, by the desktop's name, at the foot of the rail, lists, opens and
carries on the desktop's chats live through this machine's daemon, which adds
the key, and the desktop runs and saves each reply, so nothing of them is kept
here but their "New" marks. A chat there is started, renamed, edited or
deleted only on the desktop, and the margin names the device behind each turn.
One thing of such a chat can be changed from here: its Thinking choice, by the
switch in the composer's margin, which the next message sent carries to the
desktop ([Thinking](clients.md#thinking)). The switch is there only when the
desktop lists the chat's model as one that thinks, so a desktop whose gglib
predates the choice shows none and is never sent the key.

**Any other OpenAI-compatible client** on the laptop can be pointed at the
port `join` printed, `http://127.0.0.1:<port>/v1`, with this laptop's
device key as its API key. The port does not add the key for you — that is
deliberate; see [Why the port does not inject the key](#why-the-port-does-not-inject-the-key).
The key is the one this laptop was given when it paired — its own device
key, not the desktop's `proxy_api_key`, which never leaves the desktop.
`gglib remote status` here says whether this machine holds one, and
`gglib remote key --show` prints it, alone, so a script can hand it on:

```sh
curl -H "Authorization: Bearer $(gglib remote key --show)" http://127.0.0.1:8180/v1/models
```

The per-client recipes in [clients.md](clients.md) apply unchanged apart from
the port and the key.

## How it stays private

**The connection is end-to-end encrypted and the relay cannot read it.** The
tunnel is [modelpipe](https://github.com/mmogr/modelpipe) over iroh: QUIC
with TLS 1.3, keyed to the two machines' identities. When a direct path
cannot be hole-punched, a relay carries the packets — and sees ciphertext,
who is talking to whom, and how much. Never content. `--relay` moves even
that to a server you run.

**Two doors, two different keys.** A request arriving through the tunnel is
checked twice, and the checks are no longer the same check. At the edge it
must present a key the desktop issued to *that device*; nothing else is
admitted, the desktop's own `proxy_api_key` included. Past the edge the
tunnel replaces the device's key with the desktop's, so what reaches the
proxy is the credential the proxy demands and the device key stops at the
edge — a device never holds anything that would open the desktop's loopback
proxy.

That second check can therefore no longer refuse a tunnelled request on its
own: it is validating a header the tunnel wrote microseconds earlier. What
stands in its place is a check that the edge named a device at all. Only a
named key makes it do so, and the edge admits no other kind, so the check
refuses only a request whose markers were forged by a client that reached the
proxy directly, with `403 device_not_paired`. A pairing code is not a key: the
edge answers it on its own pairing route, and refuses it everywhere else with
`401`, like any key it does not hold.

Rotating the key on the desktop (`gglib config settings set
--proxy-api-key`) reaches the running tunnel within two settings-cache
windows, about ten seconds, and changes only that last hop. No device has to
re-pair, though a request made inside that window can be refused.

**A device that is refused says so flatly.** `invalid or missing bearer
token` is what the tunnel writes when a key is not admitted — naming nothing,
because at that point the tunnel is all that has looked at the request. That
is what a *third-party* client pointed at the loopback port sees. gglib's own
turns do not stop there: the daemon reads the refusal's `invalid_api_key`
code and says which machine refused and what to do about it. The cause is no
longer a rotation, which un-pairs nobody; it is that the desktop has retired
this device, or that the two are mid-rotation and the edge has a stale
credential for the proxy — that one clears itself within a few seconds.

**The key is not something to type in.** There is no
`gglib config settings set --remote-api-key`, and that is deliberate rather
than missing: the laptop's copy is written only by `gglib remote join`,
which redeems a code and stores the key *together with the ticket it came
from*. A hand-set key could name a machine the stored ticket does not, which
is exactly the desync — connected, holding the wrong machine's key, every
request refused — that keying the record by ticket fingerprint exists to make
impossible. `scripts/check_settings_surfaces.sh` records the exemption with
that reason. Pair again instead; it is one command on each side.

**Pairing moves a one-time code, not the key.** The six-digit code is
answered by the tunnel edge itself and never reaches this machine's proxy.
It lives two minutes and dies on first use, and each endpoint that presents
it gets three tries before the edge locks that endpoint out of it. It is
useless without the ticket, which is the only way to reach the edge that
answers it. The key it buys is minted for this one device and travels once,
inside the encrypted tunnel, in exchange for that code. A wrong code, a spent
or expired one, and a locked-out endpoint all get the same refusal. Somebody
who mints enough endpoints to keep guessing ends the invite instead, and this
machine logs it as burned. [The ADR log](adr/log-0012.md) keeps the history
of this paragraph's earlier corrections.

A laptop on gglib 0.18 or a phone on ggchat 0.2.4 cannot pair with a desktop
running this version, because they send the code as a key, which the tunnel
edge refuses before the proxy sees it; they report that as a refused pairing
code. A device either of them has already paired keeps working. Nor can a
laptop on this version pair with a desktop still on gglib 0.18: that desktop's
edge spends the code on the attempt and its proxy has no route for it, so the
request comes back HTTP 404 and `join` names that status and says to update
the desktop. Any other status is reported without blaming a version: 404 is
the only one anyone has traced to a mechanism, and sending an operator after a
desktop that may already be current wastes their time.

**The daemon's API asks for its token, on loopback too.** The socket at
`127.0.0.1:9887` is the machine's boundary, not yours: any process on the
machine can open it, another account's included. Through `/api` such a process
could switch the tunnel on and mint itself a device key the edge admits until
`gglib remote forget` retires it, register an MCP server whose command then
runs as you, or rewrite settings. So every `/api` route answers only the
daemon's token, and refuses anything else with `401` and a sentence that says
how to get it; `/health` stays open. The token is a random secret the daemon
mints at every start, in `<data root>/data/daemon_token`, readable only by its
owner: a new one each time, so a token something captured while the daemon was
down, by answering on its port, dies at the next start. `gglib` on this machine
reads the file for each command and sends it; the desktop app reads it for
each request of its own and hands it to its window; the dashboard gets it from
the link `gglib web` prints, `http://127.0.0.1:9887/#token=…`, and opens in your
browser (`--no-open` only prints it). The page takes it out of the address bar
and keeps it in the browser for that page's origin, so a bookmark works until
the daemon next starts, and after that the link has to be opened again. That
closes the API to another account on the machine, which can read neither the
file nor the database. It does not close it to code running as you, which can
read the file as it can read the rest of gglib's data directory. A daemon
started with `--share-lan` also takes its API key, which the LAN holds; the
proxy's key does not open a loopback daemon. Opening the link puts it on the
launcher's command line, which any account can list with `ps`, for the moment
the launcher runs: on macOS that is
`/usr/bin/open` until LaunchServices takes the link, after which it travels to
the browser by Apple Event; on Linux, if the browser was not already running,
it is the browser itself, for as long as it runs. The link then lands in the
browser's history like any pasted link. On a machine shared with other
accounts use `--no-open`, or accept that they could learn a token that dies at
the next daemon start.

**A web page is not such a process.** Your browser opens that socket for any
site you visit, but it says which site is asking, in `Origin`. Any request to
an `/api` route but a `GET`, `HEAD` or `OPTIONS` from another site is refused
with `403 ORIGIN_NOT_ALLOWED`, so a page cannot switch the tunnel, mint an invite,
disconnect, stop the daemon or touch the llama.cpp install. A site counts as
another unless it is the daemon's own or on the daemon's CORS list, so a page
that names any origin but the daemon's own may change something exactly when
the daemon's CORS lets it read the answer. The desktop
app (`tauri://localhost`, and `http://tauri.localhost` on Windows) and the dev
server at `http://localhost:5173` are on that list; the dev server opened under
another name, such as `127.0.0.1:5173`, is not, and its changes are refused. The
dashboard the daemon serves passes under any name the daemon answers to; the
CLI and other programs send no `Origin` and pass. `Origin: null` is refused,
and so is a request with no `Origin` marked `Sec-Fetch-Site: cross-site`. The
proxy on `:8080` holds the same line against its own CORS, which lets pages on
`localhost`, `127.0.0.1` and `[::1]`, on any port, and the desktop app read, so
a page elsewhere cannot run a turn, load a model or clear the cache through it
either. Nor can a browser extension: its origin, such as
`chrome-extension://…` or `moz-extension://…`, is not among them, so the proxy
refuses its changes even where the extension may read the answer, and no
setting admits one. A daemon started with `--share-lan` lets every page read,
and its token is what refuses them. Whatever serves a page on `localhost` is a
process, the case above.

**One identity, kept.** `enable` reuses this machine's stored endpoint key, so
the ticket is the same ticket every time and a device pairs once rather than
every session. `gglib remote disable` stops answering — the ticket reaches
nobody while the tunnel is down — but it does not revoke anything: `enable`
brings the same address back. Retiring one device, with `gglib remote forget`,
is the revocation. Deleting the endpoint key, which `gglib remote status`
prints the path to, retires the address and revokes no key: a device that
learns the new ticket and still holds its key is admitted. What a lasting
name costs in return is under [You pair once](#you-pair-once) above.

**Enabling puts the key on the local proxy too.** The tunnel and the proxy
are one listener, so enabling remote access makes the desktop's own loopback
proxy require the API key from then on — and disabling does not take that
away. gglib's own CLI and GUI read the key from settings and carry on; a
hand-configured local client will start getting `401` and needs the key added
once. `enable` says so every time it switches remote access on; one answered
by a session that was already up, or that the daemon put back while it
waited, switched nothing on and says that instead.

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
that is still running keeps demanding each device's own key, which clearing
`proxy_api_key` does not touch — so this opens the local door only;
`gglib remote disable`
closes the remote one. And `/mcp` *does* come back open, along with `/v1/*`,
because it sits behind the same bearer guard. That is the ordinary posture of
a loopback proxy that never enabled remote access, not a new hole, but it is
worth knowing before you unset on a machine with a shell MCP server
configured. [ADR 0012](adr/0012-the-remote-tunnel.md), decision 2, has the
mechanism.

With remote access switched on, the `gglib proxy stop` in that procedure also
takes the tunnel down, and starting the proxy again does not bring it back,
as a proxy restart otherwise would. The proxy has come back demanding no key
with none stored, so putting the tunnel back would mean minting one, which
would lock the local proxy you have just opened, with nothing to tell you. The
daemon leaves the tunnel down instead, with the switch still on, and says so
in its log. `gglib remote enable` brings it back with a new key, which locks
the local proxy again; `gglib remote disable` switches remote access off.

**The proxy, and only the proxy.** The daemon's management API on
`127.0.0.1:9887` — the door `gglib`'s own commands and the desktop app come
through — is not affected. A daemon bound on loopback, which is the default,
asks its own token and no key, whatever `proxy_api_key` says later; a daemon
started with `--share-lan` already had its key before the tunnel existed. Enabling remote access cannot lock
you out of the tool you enabled it with.

**What the network learns.** By default the desktop publishes its address to
n0's discovery service so the ticket keeps working when it changes network,
and does *not* ask the router to open a port (UPnP/NAT-PMP is off on both
sides). `--no-discovery` removes the discovery contact at the cost of
mobility; `--relay` removes the public relays. What remains — a relay
knowing that two machines talk and how much — is observability, not
readability.

## What the other machine can reach

Everything the desktop's proxy serves — `/v1/models`, a model's detail at
`/v1/models/{name}/detail`, `POST /v1/models/{name}/load`,
`/v1/chat/completions`, `POST /v1/images/generations`, `GET /v1/images/drawing`, `/v1/runs`, `/v1/chats`, `/v1/attachments`, the dashboard, `POST /v1/proxy/shutdown` —
with one exception. `/mcp`, the tool gateway, is refused over the tunnel unless the
desktop ran `enable --allow-mcp`, because a leaked key with a shell MCP
server configured on the desktop is remote code execution. The refusal is
a `403` naming the flag; local clients are unaffected. The proxy tells a
tunnelled request apart by a marker the tunnel edge sets and a peer cannot
remove. Forging it denies yourself `/mcp`, and on `/v1/runs`, where a device
sees the runs it started and every run on the desktop's chats, and `/v1/chats`,
it lets a client that reaches the desktop's proxy directly act as that device:
this machine not reading a device's reply is a courtesy of the API, not a
boundary.

A paired device may draw with the desktop's image model at
`POST /v1/images/generations`, OpenAI's Images API: `{prompt, model?, n?,
size?, seed?}` answered with `{created, data: [{b64_json}], output_format}`.
That synchronous form carries no progress, and a render takes minutes; with
`"stream": true` the answer is server-sent events, gglib's
`image_generation.progress` at every stage and step, OpenAI's
`image_generation.partial_image` up to `partial_images` times (0 to 3; a
small preview frame, whose `size` is the size asked for, not its own), and
one `image_generation.completed` per image, which also names the image
model that drew it in gglib's own `model` key. Drawing uses the desktop and
changes nothing on it, like a chat. `GET /v1/images/drawing` answers whether
the desktop can draw, `{available, code?, reason?, model?}`, with the reason
when it cannot (no image runtime, no image model, several and no default).
That answer is always a 200: its `code`, `drawing_unavailable`, is the code
a request to draw would be refused with, not an error of the question;
a device asks it before offering a Draw button, and a desktop too old to
have the route answers 404, which means it cannot.

A paired device may read the desktop's chats at `/v1/chats` and carry one on
with `PUT /v1/runs/{id}?kind=agent` and `{conversation_id, content, images?, thinking?, draw?}`: the
desktop runs the reply from its own record and saves both rows, marked with
the device's name, and its own page and every paired device can follow that
run. A chat that stored its model runs on that model by its id, the one its
last turn on the desktop used, so another of the same name cannot answer it;
one that ran on the machine the desktop is itself paired with is refused,
`409 conflict`, in a sentence that does not name that machine, and its model
is never looked up as a name in the desktop's catalogue. Nothing is copied to the device, only a named device
gets past `403 device_not_named`, and `gglib remote forget` takes the chats
away with the key, though a reply the device already started still finishes
and is saved. Such a turn calls none of the desktop's MCP tools unless the
desktop ran `enable --allow-mcp`, the same gate as `/mcp`, and then only the
tools the chat names. A saved reply's row says how it was made in its
`metadata`: beside its token counts, the context its model was launched with
(`contextSize`), how many earlier messages the run left out to fit
(`trimmedMessages`) and why the model stopped (`finishReason`), each only
when it is known, so a device that opens the chat reads them with no route
of their own ([Context reading](clients.md#context-reading)).

The `PUT` answers as soon as the run is made, before the chat's model is
loaded. A model that has to load, or wait behind an image render, shows as a
`waiting` event in the run, and a model that cannot be loaded ends the run
`failed` with `model_unavailable` rather than refusing the `PUT`; a client
shows a run's error code as it would the `PUT`'s. Such a run has written
nothing to the chat.

A turn sent with the device's Draw button pressed says `"draw": true`, and
only then is the desktop's chat model offered its image tool, for that one
message: it writes the prompt, draws with the desktop's image model, and the
picture is saved on the reply's tool row. Whether to draw is not left to the
model: the reply's first step must be the call for the picture, and a model
that answers in words instead ends the run `failed`,
`image_generation_failed`, "the model did not ask for the picture; try again
or pick a model that calls tools". This needs no `--allow-mcp`, which
still gates every MCP tool, and it holds for a chat with its tools turned
off: the button is the person's choice for that message. A turn without the
key is offered no image tool. A desktop that cannot draw refuses the turn,
`400 drawing_unavailable`, with the reason; a device asks
`GET /v1/images/drawing` first, and never sends the key to a desktop that
answers 404 there, which would refuse a body with a key it does not know.

A chat the device keeps itself can draw too. `PUT
/v1/runs/{id}?kind=chat&tools=builtin&draw=true` takes the device's
unchanged OpenAI chat request, whole history included, and runs it through
the desktop's agent loop on the model it names, with the image tool offered
for that message, whose first step must be the call for the picture as on a
desktop chat; without `draw=true` the loop runs with no tool. Images
sent inline as data URLs are stored on the desktop for the run and removed
at the first daemon start a day or more later, since no chat there links
them. Nothing is saved to any
chat on the desktop: the device keeps the conversation and reads the reply
from the run. That run's listing says `"frames": "agent"`: its events are
the agent loop's (`tool_progress`, `waiting`, `tool_call_complete` with its
images), not OpenAI chunks, and a run without the key is read as OpenAI's.
The request's sampling is not read, its `max_tokens` included; the desktop's
settings for the model apply, as on a turn. Its Thinking choice is read: a
body with `"reasoning_budget_tokens": 0`, which is how a device turns thinking
off for a chat it keeps ([Thinking](clients.md#thinking)), runs with a thinking
budget of `0`, and any other budget is sampling and is not read. Without `tools=builtin` a chat run is recorded exactly
as before.

Such a turn may also say the chat's Thinking choice, `"thinking": "off"` or
`"thinking": "default"`, and says it only when the user changes it. `off`
runs that turn with a thinking budget of `0`, and the desktop remembers it on
the chat, so every later turn there runs the same way, whichever device or
page sends it; `default` forgets it; a turn that says neither runs as the
chat remembers. An opened chat says what it remembers in its `settings`:
`"thinking": "off"`, or no such key. The choice is written when the turn's
run starts, so a turn that is refused, or that repeats a run's id, changes
nothing. No route was added for it, and it is the only setting of a chat a
turn sets by its body. Which models think is in the model list
([Thinking](clients.md#thinking)). A desktop whose gglib predates the key
refuses a turn that carries it, `400 invalid_request`, as it would any key it
does not know; it lists no model as one that thinks either, so a client that
offers the choice only for those models never sends it there.

Such a turn may carry images, and they cross the tunnel by reference. The
device sends each file once, as the raw body of `POST /v1/attachments`: a PNG
or a JPEG by its first bytes, at most 8 MiB, else `413 image_too_large` or
`400 unsupported_image`. The desktop stores the bytes exactly as they came,
under their SHA-256, and answers that id with the image's type, its size in
pixels and the prompt tokens it is estimated to cost; the same bytes sent
twice are one image and the same answer. The turn then names its images,
`{conversation_id, content, images}`, and needs text or an image. An id the
desktop does not hold is `400 attachment_not_found`, a chat whose images,
the turn's and the history's, are over 16 MiB together is `400
request_images_too_large`, and a chat whose model has no projector is `400
model_cannot_read_images`; each is answered before the model is loaded or a
row is saved. So what crosses is the file, once;
after that a turn is its text and ids, and an opened chat lists each row's
images by id, type and size, with no bytes. A device reads the bytes of one
with `GET /v1/attachments/{id}`, which answers them as they were sent, with
`no-store`, so the device keeps no image, as it keeps no chat. Both routes
are for a named device only, as `/v1/chats` is. An image no message names,
last uploaded more than a day ago, is deleted when the desktop's daemon next
starts, so one uploaded for a turn that was never sent does not stay; one a
message names stays as long as the message does.

On the joined machine the same two routes are behind its own daemon, at
`POST /api/remote/attachments` and `GET /api/remote/attachments/{id}`, which
adds the key as it does for the desktop's chats and keeps nothing. What it
reads back it serves as `image/png` or `image/jpeg` when the desktop said
so, and otherwise as `application/octet-stream`, with `nosniff` and
`no-store`. The chat
page uses both for a far chat: an image pasted, dropped or picked there is
uploaded to the desktop's store as it is added, the turn names it by the id
the desktop answered, and a turn's image is shown by reading it back through
this route. A desktop whose gglib predates images answers the upload `404`
and a turn that names one `400 invalid_request`; the page says the paired
machine's gglib cannot take images yet.

The model list carries each model's id in the desktop's catalog
(`gglib_id`) and the desktop's name (`machine_name`, its host name's first
label). A model's detail is what the desktop's own inspector shows, minus the
paths of the file and its projector on the desktop's disk and the port it
serves on; whether the model reads images is sent (`imageInput`). Reading
either changes nothing on the desktop, and `/health`, open to anyone who can
reach the port, carries neither.

A tunnelled request the edge did not admit on a *device* key reaches no
protected route: `403 device_not_paired`, before the `/mcp` gate is consulted. The
edge forwards nothing else today, so this is a second lock on the same door: a
pairing code is answered by the edge itself and refused as a key anywhere
else. Local clients are unaffected — they carry no marker.

## Why the port does not inject the key

The laptop's port could add `Authorization` to every request passing
through it, and then any client on the laptop would work without
configuration. It does not, on purpose: that would make every process on
the laptop an authenticated client of the desktop, which is a larger grant
than the one you made when you paired. gglib's own commands attach the key
because you asked them to; a third-party client supplies it as its API key,
which is the ordinary OpenAI-compatible arrangement, and
`gglib remote key --show` prints it for that client. The key in question is
this device's own, which is also what bounds the mistake: a key that leaks
here is one the desktop can retire on its own.

## Troubleshooting

| You see | It means |
|---------|----------|
| `the remote machine did not answer within 30 seconds`, or 25 when pairing | The desktop is off, offline, or has had its endpoint key deleted since. `join` binds the local port before it has reached anything, so this is the wait for first contact timing out rather than the dial failing. The ticket itself does not go stale on a restart any more; if the desktop is simply asleep, the port is released; run `gglib remote join` again once it is awake. |
| `the tunnel closed before the remote machine answered` | The local end went away while the dial was still looking. Nothing was sent through it, so the pairing code is unspent — try `gglib remote join` again with the same string. |
| `Connected: … — away 3m` in `gglib remote status` | The desktop has not answered for that long. The port here is still bound and still dialling; nothing to do but wait for the desktop, or wake it. |
| `Port 8180 was taken by something else, so this is on … instead` | The port the pairing was last reachable on is in use. The new one is remembered; point any client at it, or free the old port and `--port 8180` to pin it back. |
| `the far machine refused the pairing code` | The code was mistyped, has expired (two minutes), or was used already. A mistyped code costs only that attempt: check it and run `gglib remote join` again while the code is still on screen. Once it has gone, run `gglib remote invite` on the desktop for a new one. |
| `this machine holds no key for that remote` | You gave a bare ticket but never paired with this desktop. Use the full `<ticket>-<code>` string once. |
| `could not use the key this machine joins that remote with` | The laptop's endpoint key for that desktop, whose path the message names, is not a key, can be read by other accounts, or is not a regular file; the message gives the reason. It was not replaced. Do what the reason says — `chmod 600` a key others can read — or delete the file and run `gglib remote join` again: a new key is minted, and the desktop sees this laptop as a new endpoint. |
| `could not make the key this machine joins that remote with` | The laptop had no key for that desktop, and could not write its first one at the path the message names: a full disk, a folder this user cannot write to, or a filesystem with no hard links, for example; the message gives the reason. There is no file to delete. Fix what the reason names and run `gglib remote join` again. |
| `could not make …, where the key this machine joins with is kept` | The folder for the laptop's endpoint keys, which the message names, could not be made: a file is in its way, or this user cannot create a folder there. Move the file aside or fix the permissions, and run `gglib remote join` again. |
| `<name> is not admitting this device's key` | That machine is not admitting this device's key. Either it has retired this device, or you dialled a bare ticket for a machine this laptop never paired with. A rotation is *not* a cause any more. Invite this device again on the desktop and redeem the fresh `<ticket>-<code>`. |
| `connected to a machine this one holds no key for` | The laptop is connected to a desktop whose key it does not hold: the stored pairing is another machine's, or there is none. Every request through the tunnel, `gglib daemon stop --remote` included, is refused here before anything is sent, so another machine's key never reaches this one. Run `gglib remote invite` on the desktop and `gglib remote join` with the full `<ticket>-<code>` string. |
| `<name> runs an older gglib that publishes no model ids — update it` | `gglib model list --remote`, or the daemon's `/api/remote/models` route or one model's detail under it, asked the desktop for its models, and its gglib is from before models carried ids. Update gglib on the desktop; there is no fallback that lists them without ids. |
| `no model 7 here; desk's models need --remote (gglib model list --remote)` | The laptop has no model with that id or name, and is paired. An id from `gglib model list --remote` is the desktop's: add `--remote`. |
| `looking up '7' on desk`, caused by a 404 | `chat`, `q` or `model inspect --remote` asked the desktop for a model it does not have. Nothing was sent as a turn. `gglib model list --remote` lists what it has. |
| `this chat ran on a machine this one is no longer paired with, so it cannot be resumed here` | `gglib chat --continue` on a chat that ran on a desktop the laptop has since replaced with another pairing. Its model is that desktop's; join it again to resume the chat, or start a new one. |
| `this chat ran on this machine and continues here; drop --remote to resume it` | `--continue` with `--remote` on a chat that ran on the laptop. Leave out `--remote`. |
| `this chat ran on the paired machine, and --port names a server on this one; drop --port to resume it there` | `--continue` with `--port` on a chat that ran on the desktop. Leave out `--port`; the chat goes back to the desktop through the daemon. |
| `403 device_not_paired` | The request reached the desktop's proxy marked as tunnelled but naming no device, which the tunnel edge never sends: markers forged by a client that reached the proxy directly. A pairing code used as an API key does not get this far; the edge refuses it like any key it does not hold. |
| `invalid or missing bearer token` | The same refusal, unrendered — what a third-party OpenAI client pointed at the loopback port sees, since gglib is not in that request's path to translate it. |
| `403 ORIGIN_NOT_ALLOWED` from `:9887`, or `origin_not_allowed` from the proxy | A page on another site asked to change something. A page of your own gets this behind a reverse proxy that rewrites `Host`: pass `Host` through, and name it with `--allowed-host`. A browser extension gets it from the proxy on every change: its `chrome-extension://` or `moz-extension://` origin is not a local page, and no setting admits one. |
| `this route needs the daemon's token` | The daemon was asked for something without its token, or with one from before its last start. Run the command from `gglib` on this machine, as the account that runs the daemon, or open the dashboard from the link `gglib web` prints again. |
| `403 mcp_not_allowed_over_tunnel` | `/mcp` is closed over the tunnel. Re-enable on the desktop with `--allow-mcp` if you mean it. |
| A local client on the desktop starts getting `401` | Enabling put the key on the local proxy (`:8080`; the daemon on `:9887` is unaffected). Add the key to that client; it stays on after `disable`. |
| `gglib remote enable` says it is already enabled | The switch is already on, and nothing needs re-running to keep it that way. To pair another device, `gglib remote invite` — it offers a code against the tunnel that is up rather than refusing. To change the flags it was enabled with, `disable` first; the ticket is the same one afterwards. |
| `remote access is already being enabled` | Another `enable` is arming the tunnel; a daemon that has just started is still putting its own back after the twenty seconds commands wait for it; or the daemon is putting the tunnel back after the proxy was started again, which commands do not wait for. `gglib remote status` shows when the ticket is up; run the command again then, or `gglib remote disable` to give up on it. |
| `disable` says `Daemon is not running` | No daemon was running, so `disable` switched remote access off in settings instead, and the next start will not put the tunnel back. `gglib remote enable` turns it on again. |
| A row in `gglib remote list` reads `key held, no record` | This machine holds a key under that id and the device list has no row for it, so the id is all that is known. Nothing should leave one. It is not put on the tunnel the next time the tunnel comes up, though a tunnel that is up now may still admit it until then. `gglib remote forget <id>` retires the key. |
| A row in `gglib remote list` reads `invited …, never joined` | A code was offered for that device and nobody redeemed it. The key was minted but never transmitted, so nobody holds it; `gglib remote forget <id>` tidies the row away. Unspent invites are listed rather than swept on a timer, so that what the machine issued is always visible. |

## Not yet

Pairing over the LAN without a ticket, and pinning the pairing request to a
specific peer, are noted in the ADR's
[out of scope](adr/0012-the-remote-tunnel.md#out-of-scope) section. A phone
client is out of scope for gglib too, but it is no longer missing:
[ggchat](https://github.com/mmogr/ggchat) is one, built as its own product.
