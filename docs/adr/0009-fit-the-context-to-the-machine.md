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

**A fitted context can disqualify a model from the second slot.** The fit is
computed against the whole device, before the queue has decided which slot the
model gets; `decide_secondary_slot` then judges the result against a hard 2 GiB
ceiling. A model sized as though it owned the card is measured against a slot
that is a fraction of it.

> **Amended 2026-08-28 — measured, and it binds on the model the slot exists
> for.** `residency_fit_tests.rs` pins the arithmetic. A Qwen3-Embedding-0.6B at
> Q8_0 is about 57k KV elements per token, so at the 4096 floor its whole
> footprint is under a gigabyte and it co-resides comfortably — which is what
> the second slot is *for*. At a fitted 32,768 its KV alone is about 2 GiB and
> the ceiling refuses it. The rungs at and below 16,384 still fit; 32,768 does
> not.
>
> The consequence is not a failed launch. It is thrash: `may_co_reside` is
> false, so the embedder displaces the primary, and every `/v1/embeddings` call
> evicts the chat model while every following chat call evicts the embedder —
> two full reloads per document indexed, on the pairing this slot was added to
> keep resident.
>
> **Not fixed here, and the reason is the invariant rather than the effort.**
> The resolved context is "the single source of truth for what context this
> launch/reuse decision is about", read by the resident-match test, every log
> line and the narration — and it is resolved *before* `poll` chooses a slot.
> Sizing a candidate for the slot it will get makes that value slot-dependent,
> which means the match test in `serve` has to become slot-aware too, or a
> co-resident is recycled on its very next request for having the context it
> was granted for. That is a change to the invariant, not a change under it,
> and it wants its own decision.
>
> The operator lever that already exists is a per-model
> `server_defaults.context_length`: setting an embedder to 8192 restores
> co-residence today, because a configured value outranks the fit.

**`/v1/models` advertises the trained window when nothing is configured**, rather
than `min(trained, 4096)`. `context_window` is a gglib extension — the OpenAI
endpoint has no such field — so gglib defines its semantics, and a cap nobody
chose was the worse lie. The trained window is a true upper bound: `fit_context`
caps at it before snapping.

> **Amended 2026-08-28 — true only where the fit is reachable.** This paragraph
> assumed "nothing is configured" implies "the launch will fit the context to
> this machine". On every AMD, Intel, Vulkan and CPU-only host it does not:
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
> > **Amended 2026-08-28 — and it was not measurement-only.** The paragraph
> > above treats this as a stale instrument. It is also a live cost, because
> > `service_graph.rs` builds `benchmark_runtime` from the **same**
> > `ProcessManager` the proxy uses, and `ResidentSet::serve` recycles any
> > resident whose `context_size` differs from the one resolved. Before the fit
> > existed both sides resolved 4096 and matched; afterwards they were
> > guaranteed to differ wherever `fit_context` succeeds. So a sweep evicted the
> > proxy's resident, relaunched it at 4096, and the next chat request evicted it
> > back — a full teardown, weight reload and prefill each way.
> >
> > All three now pass the setting through and let admission resolve the chain,
> > and record what the launch actually served rather than what they asked for.
> > `scripts/check_context_floor.sh` keeps the construct out: it is the third
> > time this exact `.unwrap_or` has shipped, and each was found by reading
> > rather than by a test.
> >
> > **The re-baseline is real and is not silently absorbed.** Numbers taken
> > before this change were measured at 4096; numbers after are measured at
> > whatever the machine fits. They are not comparable, and any stored history
> > spans both.
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
>
> > **Amended 2026-08-28 — the sweep landed.** All of them now say what a
> > launch actually does. The wording is deliberately *hedged* rather than
> > merely corrected: none of these surfaces can know which rung a launch will
> > reach, so they say "sized per launch" instead of naming the fit. Naming it
> > would repeat the original error in the other direction, on the hosts this
> > ADR records as getting no fit at all.
> >
> > `ServerConfig::context_length`'s doc was the one that mattered most and the
> > one nothing would have caught: it is `ts(export)`, so a sentence claiming
> > the chain "ends in a context fitted to the machine" was compiled into
> > `ServerConfig.ts` and shipped to the GUI. The chain ends at the floor; the
> > fit is one rung above it.
> >
> > `ProxyControl.tsx` was the seven-week-old one, and wrong twice over — with
> > nothing set it offered "server default (from settings)", where there is no
> > server default and it is not from settings.

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

The first time these criteria have been evaluated against real traffic. The
values below are almost all zero and the denominators are what make them worth
recording at all, so both are stated in every line.

**Scope.** One session, ten requests, Qwen3.8-27B via VS Code Copilot, read from
`gglib proxy dashboard`. This ADR carries the provenance for the whole arc;
[ADR 0010](0010-the-loop-guard-reads-what-came-back.md) and
[ADR 0011](0011-stagnation-is-about-prose.md) cite this note rather than
repeating it.

**It cannot be re-run**, and that is a decision rather than an oversight. The
ledger's counters are per-process and reset on restart — `domain::defects`'
module docs argue for that lifetime — so no stored history exists to re-read and
this note is the only durable record of the numbers. That is weaker provenance
than "cite the reproducer" asks for. Recorded as it is rather than dressed up.

**The audit that produced this arc found 12 verified defects and 3 deferred.**
The figure of 15 that circulated in prose around it was never right. The audit is
not a repo artifact and no document held either number, so this line is the
count's only source.

- **If the chosen rung is routinely far below the unsnapped figure** —
  **not evaluated.** The reading is `gglib model explain`'s `fitted to hardware`
  against `before snapping`, taken across a catalog; the session ran neither and
  the dashboard carries neither number. **OPEN, and unread rather than clean.**
- **If launches fail at the fitted context on real hardware** — **0 failures in
  1 fitted launch, 2026-08-28.** The denominator is the whole of what this
  reading says: the dashboard reported `Model swaps  0`, so one resident served
  all ten requests and this criterion saw exactly one launch, on one host, for
  one model. Ten requests is not ten launches. **OPEN.**
- **If the reservation's one measured configuration never occurs in practice** —
  **not evaluated, and the session's one second-slot event is not evidence for
  it.** The dashboard showed the second slot refusing a ~31.6 GiB model, which is
  `decide_secondary_slot` enforcing its 2 GiB ceiling — the mechanism the
  amendment above describes, not the budget top-up this criterion is about.
  Conflating them would retire the top-up on evidence about something else. The
  reading that does exist is per model and needs no session: `gglib model
  explain`'s `device budget` against the device's nominal capacity — equal means
  the fallback to the undivided device fired, smaller means the top-up was taken.
  Nothing reports it in aggregate. **OPEN.**

**All three remain OPEN.** Each asks what happens *routinely*, and one session
cannot answer that whatever it shows. What this reading establishes is the
convention rather than a verdict: a zero is recorded as "0 across N", never as
"none", because "none" cannot distinguish a mechanism that does not fire from one
nobody exercised.

Two readings outside the criteria are kept because they bear on "Why the budget
cannot move" above. The proxy reused 58,944 of 87,941 prompt tokens and swapped
models zero times across the ten requests. A budget that drifted between requests
would have recycled the resident and taken the prefix cache with it; that neither
happened is consistent with the decision, at a sample of one session.
