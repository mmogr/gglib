# ADR 0009 — Fit the context to the machine, from a budget that cannot move

- **Status:** Accepted
- **Date:** 2026-08-26 (amended 2026-08-26 — see the notes under "Existing users
  need no migration")
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md),
  [ADR 0006](0006-recover-dont-predict.md)
- **Supersedes:** nothing
- **Superseded by:** nothing

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
rejects the session ([ADR 0006](0006-recover-dont-predict.md)'s postscript
records the counters that make that visible).

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

**A machine whose VRAM gglib cannot read gets no fit at all.** AMD, Intel,
Vulkan and CPU-only hosts fall through to the floor. Deliberate: the
recommendation module documents its own RAM fallback as "usually too generous",
and sizing a KV cache against 64 GiB of host memory on an 8 GiB card is the
working-but-unusably-slow outcome that module exists to prevent. It would also
look perfectly stable while measuring the wrong device.

**Served context is non-monotonic in device size.** A device that cannot host a
co-resident gives its primary the whole machine; one that can yields room for
one. In the narrow band where that flips, a larger device serves a smaller
context. No current hardware occupies it — GPU VRAM comes in 4/6/8/10/12/16/24
GiB and Apple unified memory is 0.75× of 8/16/24/32/64 GB — but it is a real
discontinuity in the function and it is named in the code.

**`/v1/models` advertises the trained window when nothing is configured**, rather
than `min(trained, 4096)`. `context_window` is a gglib extension — the OpenAI
endpoint has no such field — so gglib defines its semantics, and a cap nobody
chose was the worse lie. The trained window is a true upper bound: `fit_context`
caps at it before snapping.

**Existing users need no migration.** A stored global default means the user
typed a number: the settings modal shows an empty box when unset and writes back
blank, and the repository deletes the row for `null`. ~~The exception is
`gglib config settings reset`, which writes `Some(4096)` and therefore pins that
user above the fitted rung — a semantic contradiction in `reset`, recorded here
and not fixed by this change.~~

> **Amended 2026-08-26.** The exception is closed. `Settings::with_defaults`
> now leaves `default_context_size` unset, so a reset writes no row and the
> fitted rung stays reachable. A second serving path carried the same
> fabrication and was not named above: `gglib proxy` pre-resolved the chain to
> a bare `u64` and sent the floor as though the user had chosen it, which the
> daemon's own `BuiltInDefault → None` filter could not see because the value
> arrived at the explicit rung. Both now pass the setting through untouched, as
> `gglib up` already did, so the claim above — that a stored global default
> means the user typed a number — now holds for every path that serves a model.
>
> Other places still fabricate the floor, and they predate this amendment
> rather than being introduced by it — unset was already the ordinary state for
> anyone who never typed a number.
>
> The benchmark harness is one. `benchmark/agentic.rs`,
> `benchmark/compare.rs` and `benchmark/tune/mod.rs` each read the setting and
> `.unwrap_or(DEFAULT_CONTEXT_SIZE)` before resolving, so a measurement taken
> with nothing configured is taken at 4096 and never at the fitted rung. That
> means ADR 0006's retained instrument measures a configuration the product no
> longer ships. Fixing it is its own change: a benchmark that silently
> re-baselines is worse than one known to be stale.
>
> `gglib serve`'s banner is another, and it is the more embarrassing of them
> because it is the failure this ADR opens with. `launch_options.rs` builds
> `PinnedLaunch::effective_ctx` with `resolve_context_size` and no `fitted_ctx`
> — that field is only ever set at admission — so with nothing configured the
> banner prints `Context size: 4096 (resolved)` while `to_proxy_config` sends
> `None` and the daemon serves the fitted rung. The number shown and the number
> used are unrelated again, on a different surface. Making them agree means the
> banner learning what admission decided, which is a plumbing change this
> amendment records rather than makes.
>
> The GUI's memory-fit badge is the third. `useSystemMemory.ts` reads
> `settings?.defaultContextSize ?? 4096` to estimate whether a model fits.
> Before the reset fix an unconfigured user had `Some(4096)` stored and the
> estimate matched what was served; unset is now the ordinary state, so the
> badge sizes KV at 4096 while the launch fits a larger window. It understates
> rather than overstates, and `fit_context` still sizes the real launch against
> real capacity, so this misleads a person rather than risking an OOM. Named
> here because an inventory that lists two of three is worse than one that
> lists none.
>
> One more defeats the fit from the other direction, and is the one worth
> acting on first. ~~`useServerActions.ts`'s Serve action falls through to
> `model.contextLength` — the GGUF's *trained* window — when neither a custom
> value nor a stored default exists, and sends it as an explicit
> `contextLength`. That lands on the top rung, above the fit, so a GUI user
> who serves a model with nothing configured gets the full trained context
> whether or not it fits: the failure the floor at least never had. Fresh
> installs already reached it, because an unset default has always been
> `None` at that branch; the reset fix above widens the set to anyone who has
> run `settings reset`.~~ Sending nothing and letting admission decide is the
> fix ~~, and it is not made here~~ — but unlike the three above, this one can
> refuse to load rather than merely under-serve.

> **Amended 2026-08-26.** The Serve action is fixed. It now sends a
> `contextLength` only when the user typed one, so with the box empty the
> daemon resolves the whole cascade itself: the fit where it can read the
> device, and the floor on the hosts named above where it cannot. The ladder
> is unchanged and still ranks a typed value above a stored default and a
> stored default above the fit, because both of those mean a person chose a
> number. What changed is that the Serve action no longer pre-empts it.
>
> A second rung was defeated by the same expression and is closed with it.
> When nothing was typed but a global default *was* stored, the action sent
> that default as an explicit `contextLength`, where it outranked the model's
> own `server_defaults.context_length` one rung below — so a model configured
> for 16,384 was served the global 8,192 instead. The daemon already reads
> that setting into `globals.default_ctx` itself, so sending it changed no
> resolved number except in that one case, where it changed it for the worse.
> The pinned path in the same function had always been right about this and
> said so in a comment; the bare path now agrees, and both send the typed
> value alone. `settings` had no other reader in that hook and is no longer a
> prop of it, which a compile-time assertion in its test now holds open —
> `request.context_length` also reaches `admit` as `num_ctx`, but that is not
> a second channel: `resolve_launch_opts` folds it straight back in with
> `opts.context_size = num_ctx.or(opts.context_size)`, the explicit rung.
>
> The serve modal's placeholder had to move with it, because it was a promise
> about what leaving the box empty would do and the answer changed. It read
> `Model max: N`, which was true only while the action sent the trained
> window. It now names the rung that will answer while that rung is a fact
> about the model or the settings, and otherwise says `Sized per launch`.
>
> It deliberately does **not** say "fitted to this machine", and two wrong
> drafts are the argument for that. Whether the fit is reachable is not a fact
> the client holds: `fit_context` returns `None` unless it can read the device
> budget, the weight size and the KV geometry, and again when the weights
> alone exceed the budget — and this ADR records above that AMD, Intel, Vulkan
> and CPU-only hosts get no fit at all. Naming the fitted rung would have been
> a fresh false promise to every one of those users, on the surface being
> corrected for making false promises. `gpuMemoryBytes` reaches the GUI
> already and would answer the hardware half, but none of the rest. The ladder
> moved to `contextPlaceholder.ts` so its answers could be asserted rather
> than rendered and eyeballed; both wrong drafts read correctly by inspection
> and were caught only by tracing each branch to the rung it lands on.
>
> **This is the Serve action only.** A phrase sweep found the same promise on
> eight further surfaces — the inspector's Context Length row and its edit
> form, the proxy panel's context box, Settings → Default Context Size,
> `gglib config settings show`, two clap help strings, the
> `--default-context` validation error — plus a doc on `ServerConfig` that is
> `ts(export)` and was shipping a false sentence into the GUI binding. None is
> corrected here. They are a different change with a different blast radius,
> and one of them predates this arc by seven weeks, so a sweep scoped to any
> one diff would go on missing it. Recorded rather than fixed, deliberately:
> the alternative was a three-line fix carrying thirty-two files.

## Kill criteria

- If the `debug` fit log shows the chosen rung is routinely far below the
  unsnapped figure, the ladder is too coarse.
- If launches fail at the fitted context on real hardware, `BUDGET_UTILISATION`
  is too generous for this duty and wants its own constant.
- If the reservation's one measured configuration never occurs in practice, the
  second slot's top-up is complexity without a customer and should be deleted
  along with the fallback and the non-monotonic step it creates.
