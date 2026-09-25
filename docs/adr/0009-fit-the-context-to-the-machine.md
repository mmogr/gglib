# ADR 0009 — Fit the context to the machine, from a budget that cannot move

- **Status:** Accepted
- **Date:** 2026-08-26 (amended 2026-08-26 — see the notes under "Existing users
  need no migration")
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md),
  [ADR 0006](0006-recover-dont-predict.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** [log-0009](log-0009.md)

## Context

`gglib up` chooses a model by asking what fits this machine's VRAM at 32k, and
prints the answer as the number that earns the user's trust — "needs 18.9 GiB of
24.0 GiB VRAM · at 32768 context", under a comment in `up/choose.rs` saying that
arithmetic "is the part that earns trust". It then started the proxy, which
served **4096**.

The context chain had four rungs and its lowest *reachable* one was a flat 4096.
Nothing read the GGUF's trained `context_length` or the machine's memory, so the
number shown and the number used were unrelated.

The rungs below the global default were unreachable because the floor was
laundered into it. `resolve_launch_opts` assigned
`opts.global_default_ctx = Some(default_ctx)` unconditionally from a
non-optional `u64`, and every caller pre-resolved the setting before the model
was known — so every launch arrived claiming the user had chosen 4096, whether
or not anyone had. Both the built-in default and anything below it were dead
code.

4096 is also precisely the regime where an agentic client's replayed history
overflows, truncation fires, the model loses the thread, and the loop guard
acts on the session — a 400 when this was written, a forwarded note since
#1052 ([ADR 0006](0006-recover-dont-predict.md)'s postscript records the
counters that make that visible).

## Decision

**Serve a context fitted to the model and the machine, resolved from a budget
that is a per-process constant.**

Three parts, and the third is the one that took work:

1. **A fifth rung.** `ContextSizeSource::FittedToHardware` sits below a global
   default the user actually set and above the built-in floor. It is computed by
   `gglib_core::domain::fit_context` from the model's trained context, its
   weights, its KV shape and a memory budget.

2. **Refusal, not estimation, wherever an input is unknown.** `fit_context`
   returns `None` — falling through to the floor — on an unknown trained
   context, an unknown or zero weight size, an unknown KV shape, or an
   unreadable device memory. `0` weights is the sharp case: it is
   `total_model_bytes`' documented sentinel for "could not be read", and taking
   it literally hands the entire budget to the KV cache.

3. **The budget is total device capacity less a fixed reservation, and nothing
   else.** Not a live free-memory reading, and not the current resident set.

## Why the budget cannot move

The fitted value becomes part of a resident's identity: `ResidentSet::serve`
evicts and relaunches any resident whose `context_size` differs from what the
request resolves to. So a budget that drifts between requests recycles the model
it just sized, costing a full teardown, weight reload and prompt re-prefill —
and blowing the prefix cache and every saved slot file on the way.

That is the same trade [ADR 0001](0001-runtime-capability-tiers.md) records for
static arbitration: *a dynamic system could notice, but then a failure depends on
when the evidence arrived*. Here the failure is not a wrong answer, it is a
correct answer that changes.

Three designs were tried and rejected, in order:

- **A live free-VRAM reading.** Moves with whatever else the user has open — on
  Apple it is a fraction of *available* system RAM. Recycles constantly.
- **Netting out the current resident set.** Measured: it netted out the model
  being admitted, so the fit fell a rung per request until it settled on the
  floor, taking a kill-and-relaunch with it each time. It also made one model's
  budget depend on whether *another* model's KV shape was readable. Worse, the
  admission path recomputes this on every request, so this was not a launch-time
  quirk — it was the steady state.
- **A flat subtraction for the second slot.** Measured across five model shapes
  and eight devices: of the twenty-six configurations producing a fit, it
  changed the co-load verdict in **one**, and in four others cost the primary
  between one and four rungs for a secondary that either fitted anyway or could
  not fit regardless. `BUDGET_UTILISATION` already leaves a tenth of the device
  free, so a flat reservation double-counted on any device large enough to host
  a full-ceiling secondary.

What ships is a **top-up**: the reservation takes only the shortfall between the
utilisation margin and what `decide_secondary_slot` actually needs to admit a
full-ceiling secondary. Both factors are read from the constants they stand for
rather than copied, because two rounds of arithmetic errors came from
hardcoding them.

## Is this prediction? (ADR 0006)

[ADR 0006](0006-recover-dont-predict.md) says *spend effort recovering from bad
output, not predicting the configuration that avoids it*, and its decision 2
preserves "a claim about the model, not a claim about last week's traffic" as
legitimate.

The arithmetic is that kind of claim. Trained context, KV shape and weight size
are GGUF facts; device capacity is a hardware fact; the multiply is how
llama.cpp actually allocates its unified KV cache at `--parallel 1`. Condemning
it would also condemn `SlotFootprint`, `compute_auto_cache_ram_mb` and `up`'s own
recommendation.

**The honest weakness is elsewhere and is recorded rather than argued away.**
`BUDGET_UTILISATION = 0.9` was written for a one-shot, *user-confirmed* model
recommendation at a fixed 32k. It now governs an *automatic, unconfirmed,
per-launch* allocation at up to 128k, where one of the terms it absorbs — the
compute buffer — scales with context and the margin does not. It is the
weakest-grounded constant in this change, and it is load-bearing for the only
failure mode the refusals do not close.

There is also **no recovery arm**: a fit too large to load fails deterministically
and reproduces identically on every retry, because the budget is a constant. The
mitigations are that the failure is loud (llama-server aborts at startup and the
launch guard reaps the child), the error names the fitted context and the escape
hatch, and `GGLIB_DISABLE_CONTEXT_FIT` turns the rung off. A rung-drop-and-relaunch
would be the ADR 0006-shaped answer and is deliberately not built here.

## Tier (ADR 0001)

**Tier B.** llama-server has no catalog and no notion of co-residents, so it will
never make this decision; deciding it is structurally gglib's. Tier B owes no
deletion criterion. `fit_context`'s inputs and chosen rung are logged at `debug`
as a diagnostic — a person's record of what the two judgement calls produced, not
an instrument anything acts on.

## Consequences

**Every user who never set a global default gets a different context on their
next launch.** That is the point, and it is also the blast radius.

~~**A machine whose VRAM gglib cannot read gets no fit at all.** AMD, Intel,
Vulkan and CPU-only hosts fall through to the floor.~~

> **Amended 2026-08-28 — the population shrank.** Readings:
> [log-0009, 2026-08-28](log-0009.md#amended-2026-08-28--the-population-shrank)

The original reasoning, which still governs the hosts that remain. Deliberate:
the recommendation module documents its own RAM fallback as "usually too generous",
and sizing a KV cache against 64 GiB of host memory on an 8 GiB card is the
working-but-unusably-slow outcome that module exists to prevent. It would also
look perfectly stable while measuring the wrong device.

**Served context is non-monotonic in device size.** A device that cannot host a
co-resident gives its primary the whole machine; one that can yields room for
one. In the narrow band where that flips, a larger device serves a smaller
context. No current hardware occupies it — GPU VRAM comes in 4/6/8/10/12/16/24
GiB and Apple unified memory is 0.75× of 8/16/24/32/64 GB — but it is a real
discontinuity in the function and it is named in the code.

**A fitted context can disqualify a model from the second slot.** The fit is
computed against the whole device, before the queue has decided which slot the
model gets; `decide_secondary_slot` then judges the result against a hard 2 GiB
ceiling. A model sized as though it owned the card is measured against a slot
that is a fraction of it.

> **Amended 2026-08-28 — measured, and it binds on the model the slot exists
> for.** Readings:
> [log-0009, 2026-08-28](log-0009.md#amended-2026-08-28--measured-and-it-binds-on-the-model-the-slot-exists-for)

**`/v1/models` advertises the trained window when nothing is configured**, rather
than `min(trained, 4096)`. `context_window` is a gglib extension — the OpenAI
endpoint has no such field — so gglib defines its semantics, and a cap nobody
chose was the worse lie. The trained window is a true upper bound: `fit_context`
caps at it before snapping.

> **Amended 2026-08-28 — true only where the fit is reachable.** This paragraph
> assumed "nothing is configured" implies "the launch will fit the context to
> this machine". On a CPU-only host, and (until the Vulkan probe landed later
> the same day) on every AMD, Intel and Vulkan host, it does not:
> `total_device_memory_bytes` returns `None`, `fit_context` refuses, and the
> chain lands on the floor. Composed with #926 leaving `default_context_size`
> unset, those hosts advertised the trained window — up to 131,072 — while
> llama-server was launched at 4096.
>
> That is a **regression** rather than a known limitation: before this ADR the
> same host advertised `min(trained, 4096)`, which was honest. A Copilot picker
> reads the endpoint once, so it then budgets a session against a window the
> server cannot hold.
>
> The advertisement now takes the fit's availability as an input. The runtime
> reads it — `gglib-proxy` has no device probe and cannot depend on the crate
> that does — and where no fit is reachable the floor is advertised, because the
> floor is what a launch actually gets.
>
> **Per-model refusals are not covered.** `fit_context` also declines on an
> unknown KV shape or weights that exceed the budget, and those are properties
> of one model rather than the machine. A single boolean cannot express them, so
> such a model still advertises its trained window on a host that can otherwise
> fit. Narrower than the systemic case this closes, and recorded rather than
> silently left.

**Existing users need no migration.** A stored global default means the user
typed a number: the settings modal shows an empty box when unset and writes back
blank, and the repository deletes the row for `null`. ~~The exception is
`gglib config settings reset`, which writes `Some(4096)` and therefore pins that
user above the fitted rung — a semantic contradiction in `reset`, recorded here
and not fixed by this change.~~

> **Amended 2026-08-26.** The exception is closed. Readings: [log-0009, 2026-08-26](log-0009.md#amended-2026-08-26--the-exception-is-closed)

> **Amended 2026-08-26.** The Serve action is fixed. Readings: [log-0009, 2026-08-26](log-0009.md#amended-2026-08-26--the-serve-action-is-fixed)

## Kill criteria

- If the chosen rung is routinely far below the unsnapped figure, the ladder is
  too coarse. `gglib model explain` prints both, per model, without launching
  anything — so this is now a survey rather than a log to grep. (Amended
  2026-08-28: it was a `debug!` line emitted after a launch and collected by
  nothing, which made this criterion unreadable in practice.)
- If launches fail at the fitted context on real hardware, `BUDGET_UTILISATION`
  is too generous for this duty and wants its own constant.
- If the reservation's one measured configuration never occurs in practice, the
  second slot's top-up is complexity without a customer and should be deleted
  along with the fallback and the non-monotonic step it creates.

### First reading, 2026-08-28

Readings: [log-0009, 2026-08-28](log-0009.md#first-reading-2026-08-28)
