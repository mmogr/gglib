# remote

<!-- module-docs:start -->

`gglib remote` — the tunnel that puts one machine's proxy on another
([ADR 0012](../../../../../docs/adr/0012-the-remote-tunnel.md)). Mostly thin
clients over `/api/remote/*` on the daemon; `key` reads the stored pairing
from settings instead. The tunnel itself lives in
`gglib-app-services::RemoteOps` and this crate never sees it.

# Module Layout

```text
remote/
  mod.rs          — status, and the shared status printer
  disable.rs      — `gglib remote disable`: take the tunnel down, or with no
                    daemon running, switch it off so the next start does
                    not put it back
  enable.rs       — `gglib remote enable`: bring the tunnel up; `--invite`
                    also pairs a device, so a first run is one command
  invite.rs       — `gglib remote invite`: pair one more device, changing
                    nothing about the session the others are using
  devices.rs      — `list`, `forget`: who may use the tunnel
  pairing_tui.rs  — the pairing screen: QR + code in the alternate buffer,
                    gone the moment a device pairs, the code expires, or
                    the invite is withdrawn
  connect.rs      — `join` (and `connect`, its old name): this machine as
                    the laptop, plus `disconnect`
  key.rs          — `gglib remote key`: this device's key, printed on
                    stdout alone under `--show`, for a client that is not
                    gglib
```

# Who may use it

Every device that pairs gets a key of its own, so `forget` retires one and
leaves the rest connected. The list has to distinguish two rows that look
alike: a device that paired and has not made a request yet, and an invite
nobody ever redeemed. **A row is called never-joined only when it has neither
`redeemed_at` nor `last_seen`** — both are written by background tasks and
either can be lost, so one alone would eventually libel a real device.

`invite` needs the tunnel up, and waits for one the daemon is putting back
after a start. It leaves it untouched: the flags it was enabled with, the
ticket, and every device already on it. `enable --invite`
remains, because a first run should not be two commands.

# The other side

`join` takes the string `invite` showed on the other machine. `connect` is
the name it had and still reaches the same handler for one release, printing
a `note:` that says so. With the
`-<code>` suffix it is a first pairing: the daemon dials the ticket, redeems
the code through the tunnel for that machine's API key, and stores both the
key and the ticket. Without it the stored key is used; with no argument at
all the stored ticket is dialled. The daemon reports the loopback port that
is now the far machine, and this prints it with the reminder that a client
pointed there supplies the key itself — the port does not inject it.

`key --show` is where such a client gets it. Stdout carries the key and
nothing else, so `$(gglib remote key --show)` is exactly the key; the warning
that it is a secret goes to stderr, where `key` never writes the key. A bare
`key` prints nothing on stdout and says how to ask, and with no pairing stored
both fail.

Stopping the far machine is `gglib daemon stop --remote` (ADR 0013): a
`daemon` command pointed at another machine, beside the local stop, asking
the same question first.

# What `enable` shows, and where

`enable` is the one moment the ticket and the pairing code exist on a screen.
They are drawn in the terminal's **alternate screen buffer**, the way `less`
draws, so leaving it restores whatever was there and nothing is left in the
scrollback for a later screenshot or `tmux` history to find. The screen polls
`GET /api/remote/status` once a second and leaves as soon as the daemon
reports a device paired or the code gone, or when the code expires — an
unattended terminal showing a credential indefinitely is the case this exists
to avoid. A code the daemon lets go of early has to be read gone twice
running, because a redemption clears the code before it records the pairing,
and one that goes in the last two seconds is counted as the expiry it was
about to be.

`--no-qr`, or a stdout that is not a terminal, prints the pairing string as
plain text instead and returns. The string is a credential for two minutes;
the plain path is for scripts and for terminals that cannot draw a QR, not a
convenience.

# The notice

Enabling remote access puts a bearer requirement on the *local* loopback proxy
too — it is one listener — and disabling does not take that away. `enable`
says so every time it switches remote access on, because a hand-configured
local client will start getting `401` and the person reading this is the one
who has to add the key. An `enable` answered by a session that was already
up, or that the daemon put back while it waited, switched nothing on and says
that instead.

The daemon's management API on `127.0.0.1:9887` is a different listener and is
not touched. It settles its own token at bind — none for a loopback daemon —
so nothing `enable` writes to `proxy_api_key` can close the door this CLI
itself comes through. The notice says so too, because the failure it describes
would otherwise look exactly like the one it is warning about.

<!-- module-docs:end -->
