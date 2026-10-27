# ADR 0027 — A stored secret is shown only when asked for

- **Status:** Accepted
- **Date:** 2026-10-27
- **Depends on:** [ADR 0012](0012-the-remote-tunnel.md)
- **Supersedes:** two sentences of
  [ADR 0012, decision 2](0012-the-remote-tunnel.md#2-one-credential-checked-at-two-doors)'s
  2026-09-07 amendment: the recovery it names, `gglib config settings show`
  printing the proxy key unmasked, and "Emptying the field in the desktop
  app's settings sends `null` and clears it the same way `unset` does".
  Nothing else in it.
- **Superseded by:** nothing
- **Log:** none

## Context

gglib keeps three secrets as plain text in `gglib.db`: the proxy key
(`proxy_api_key`), the key `gglib remote join` stores (`remote_pairing`), and
MCP server env values, which are base64-encoded rather than encrypted. File
permissions protect them on disk. They got out through what gglib printed and
returned ([#1578](https://github.com/mmogr/gglib/issues/1578)):

- `gglib config settings show` and `settings set` printed the proxy key.
  ADR 0012 called that the recovery (decision 2's 2026-09-07 amendment), and
  a later amendment of the same decision (2026-09-11, item 5) kept device
  keys out of settings because that output is what people paste into bug
  reports.
- `gglib up` printed it, and so did a `--share-lan` daemon, in the terminal
  it was started from. A banner is output people paste into issues too.
- `GET /api/config/settings` returned it, and the GUI showed it in a plain
  text field.
- Every MCP server response returned each env value in full.

## Decision

### 1. A read says whether a secret is held, never what it is

Settings carry `has_proxy_api_key`, as the remote status already carries
`has_remote_key`. An MCP server is answered with its env keys alone. The CLI
prints `(set, not shown)` and the command that reveals it; the GUI shows
`••••••••`.

### 2. A write that leaves a secret out keeps it

A settings update already worked this way: an absent field is left alone and
`null` clears it. The settings dialog now sends `null` only for its Clear
control; a field left empty sends nothing. An MCP env entry sent with no
value keeps the value stored under its key, and a blank value is refused, so
an emptied field cannot delete a secret by accident.

### 3. One reveal

`SecretsService::reveal` in `gglib-core` is the only code that hands a stored
secret back to a person or a client. Two callers use it:
`gglib config settings reveal <key>`, which writes the value alone on stdout,
and `POST /api/secrets/reveal`, behind the daemon's token and origin guards,
which the GUI uses. The route refuses the pairing key: it is this device's
credential for another machine, and a client on the network holding the
proxy key must not be able to read it. `gglib remote key --show` is removed,
and its stdout contract moves to `reveal`.

### 4. A key the daemon minted and could not store is printed once

A daemon started `--share-lan` mints a key when none is stored. If it then
fails to store it, its banner is the only copy, so the banner prints it. A
key that was stored is never printed: the banner says where it came from and
names the reveal command.

A proxy bound off loopback prints no banner of its own. A key it mints and
cannot store guards that run and is printed nowhere; the daemon presents it
to that proxy itself, and a client has to be given a key set with
`gglib config settings set --proxy-api-key`.

## Consequences

- The recovery ADR 0012 relied on is now
  `gglib config settings reveal proxy-api-key`. It prints the key alone, so
  `$(gglib config settings reveal proxy-api-key)` is exactly the key, and the
  output of `settings show` is safe to paste.
- What breaks:
  - `gglib remote key` is gone;
  - the settings DTO's `proxyApiKey` is replaced by `hasProxyApiKey`;
  - MCP server responses carry `env_keys` in place of `env`, and an update
    may leave an entry's `value` out;
  - a blank env value is refused, so a template's variable left empty, which
    used to save as `""`, now has to be filled in or removed.
- The GUI asks the daemon for the proxy key:
  - when the dashboard opens, and again when the settings change while it
    is open;
  - when the proxy popover opens on a running proxy, whose Clear Cache
    presents that key, and again when the settings change;
  - while the tray panel shows a running proxy;
  - at each Show or Copy.
- The values are still plain text at rest. File permissions remain their
  only protection on disk.

## Kill criteria

- An issue that reports a key lost because no surface would show it, which
  `reveal` could not return. The reading is
  `gh issue list --repo mmogr/gglib --state all --search 'lockout OR "lost the key" OR "settings reveal" in:title,body'`.
  On 2026-10-27 it returned 2 issues, #1578 (this change's own) and #963 (the
  tunnel's); neither reports a lockout.

## Out of scope

- The daemon token in the link `gglib web` prints.
- An OS keychain, or any other protection at rest.
- A redacting `Debug` or `Serialize` on the settings types. A masking
  `Serialize` would corrupt the settings stored through serde.
