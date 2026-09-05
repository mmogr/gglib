# remote

<!-- module-docs:start -->

`gglib remote` — the tunnel that puts one machine's proxy on another
([ADR 0012](../../../../../docs/adr/0012-the-remote-tunnel.md)). Thin clients
over `/api/remote/*` on the daemon; the tunnel itself lives in
`gglib-app-services::RemoteOps` and this crate never sees it.

# Module Layout

```text
remote/
  mod.rs          — disable, status, and the shared status printer
  enable.rs       — `gglib remote enable`: bring the tunnel up, show the pairing
  pairing_tui.rs  — the pairing screen: QR + code in the alternate buffer,
                    gone the moment a device pairs or the code expires
  connect.rs      — `connect`, `disconnect`, `kill`: this machine as the laptop
```

# The other side

`connect` takes the string `enable` showed on the other machine. With the
`-<code>` suffix it is a first pairing: the daemon dials the ticket, redeems
the code through the tunnel for that machine's API key, and stores both the
key and the ticket. Without it the stored key is used; with no argument at
all the stored ticket is dialled. The daemon reports the loopback port that
is now the far machine, and this prints it with the reminder that a client
pointed there supplies the key itself — the port does not inject it.

`kill` is the one-way door. It asks for the word `shutdown` on a terminal
(`--yes` skips the question; a non-terminal stdin does too, since a script
that passes `kill` has read the help) and then the far daemon stops entirely
— proxy, models, downloads — and nothing restarts it from here.

# What `enable` shows, and where

`enable` is the one moment the ticket and the pairing code exist on a screen.
They are drawn in the terminal's **alternate screen buffer**, the way `less`
draws, so leaving it restores whatever was there and nothing is left in the
scrollback for a later screenshot or `tmux` history to find. The screen polls
`GET /api/remote/status` once a second and leaves as soon as the daemon
reports a device paired, or when the code expires — an unattended terminal
showing a credential indefinitely is the case this exists to avoid.

`--no-qr`, or a stdout that is not a terminal, prints the pairing string as
plain text instead and returns. The string is a credential for two minutes;
the plain path is for scripts and for terminals that cannot draw a QR, not a
convenience.

# The notice

Enabling remote access puts a bearer requirement on the *local* loopback proxy
too — it is one listener — and disabling does not take that away. `enable`
says so every time, because a hand-configured local client will start
getting `401` and the person reading this is the one who has to add the key.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
| [`connect.rs`](connect.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-remote-connect-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-remote-connect-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-remote-connect-coverage.json) |
| [`enable.rs`](enable.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-remote-enable-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-remote-enable-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-remote-enable-coverage.json) |
| [`pairing_tui.rs`](pairing_tui.rs) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-remote-pairing_tui-loc.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-remote-pairing_tui-complexity.json) | ![](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-cli-remote-pairing_tui-coverage.json) |
<!-- module-table:end -->

</details>
