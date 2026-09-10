# ADR 0013 — The target is a value: one `--remote`, decided in one place

- **Status:** Accepted
- **Date:** 2026-09-10
- **Depends on:** [ADR 0012](0012-the-remote-tunnel.md)
- **Supersedes:** nothing
- **Superseded by:** nothing

## Context

ADR 0012 put one machine's proxy on another. Using it from the CLI was a
flag, `--remote`, and the flag was a special case: declared on the two
commands that compose an agent session, fused into the same struct as
`--port` (which names a llama-server *here*), and threaded by hand as a
`bool` through six `if remote` branches under `handlers/agent_chat/` — one
deciding the model name on the wire, one whether this catalogue is looked
up, one what the request pipeline shapes, two around profile selection on a
resume, one whether the default model applies. Every remote-aware behaviour
meant finding them all, and what leaked out was a set of rules a person had
to memorise: with `--remote` the model was mandatory, `--profile` was
refused, and `gglib chat 7 --remote` sent the literal `"7"`.

None of those were rules about remoteness. They were places where the
special case had not been finished, and the shape guaranteed there would
always be another.

## Decision

### 1. One flag, declared once

`--remote` is a global argument on the root parser, so it is accepted after
every subcommand and before any of them, in the same words. The parser does
not know which commands it means anything to; that is `dispatch`'s to say.

### 2. Which machine is a value, and it is decided in one place

The flag becomes a `Target` — `Local` or `Remote` — and every question whose
answer depends on which machine a turn runs on is a method on it, in
`crates/gglib-cli/src/target.rs`: which upstream, whether this machine's
catalogue applies, how the request pipeline shapes the turn, what the model
on the wire is called, what a turn is for when nobody named a model. A
handler holds a `Target` and calls; it does not test it. The one file that
tests it besides `target.rs` is `profile_selection.rs`, which owns the
profile policy and is where the arm for each machine belongs.

Adding a machine a command could run on is an arm in each of those methods.
It is not a search.

### 3. What `--remote` reaches is a table, and it is opt-in

`target::reach` maps every `Commands` variant to *uses a machine* or *is
about this one*, exhaustively, so a new command does not compile until it
has said which. The default is the safe one: using the paired machine is
opted into per command, never inherited. A command that is about this
machine refuses `--remote` with one sentence, naming itself and what the
flag does reach, rather than ignoring the flag — a flag that is silently
ignored on some commands is the confusion this ADR exists to end.

The line the table draws is: **use the machine, don't change it.** Over the
pairing a command may use what is on the other machine — a turn, its
catalogue, its proxy's status, stopping it. It may never change what is on
it: no pulling or removing models, no writing settings. The reasons are in
ADR 0012's context (a leaked key must not be a leaked machine) and one more:
settings hold the key and the bind, and a machine whose settings can be
changed through its own tunnel can be locked away from its owner. Today the
table reaches `chat` and `q`; the rest of the *use* side follows.

### 4. A turn remembers the model it asked that machine for

`RemotePairing` gains `default_model`: the model this machine last asked
the paired machine for. A `--remote` turn that names a model stores it; one
that names none uses it; one that names none before anything is remembered
is refused here, with the list to choose from, rather than answered
`404 Model '' not found` from the other end. Remembered rather than
configured — there is no flag or setting — because the model a person asks a
machine for is the one they asked it for last time, and per pairing because
it is a name in that machine's catalogue.

## Costs

- The remembered model is *last asked for*, not *last that worked*. A name
  that 404s is remembered and 404s again with the same clear message until
  another is named. That is a smaller surprise than a turn silently running
  on a different model.
- Every new command declares its reach. That is the point, and it is one
  line.
- `--remote` on a command that stays local is now an error where it used to
  be silently accepted by the two commands that had the flag and a parse
  error on every other. Both were worse.

## Out of scope

- A sticky target (`gglib use <machine>`) that makes every following command
  remote. Considered and declined: it hides state a person has to remember,
  and the daily command is `gglib chat --remote`, which is short.
- Named remotes (`--remote <name>`). One pairing is stored; a second serving
  machine is a settings change and a flag argument, not a redesign.
