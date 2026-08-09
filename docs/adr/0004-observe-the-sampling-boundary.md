# ADR 0004 — Observe the sampling boundary

- **Status:** Accepted
- **Date:** 2026-08-09 (amended 2026-08-09 — finding 1 overstated which
  launch paths pass sampler flags, and named launch flags as the only thing
  that masks `/props`; see the amendment there and finding 7)
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md),
  [ADR 0003](0003-defer-sampler-defaults-to-llama-cpp.md)
- **Supersedes:** nothing
- **Superseded by:** nothing

## Context

[ADR 0001](0001-runtime-capability-tiers.md) sorts every module into three
tiers and says of the third:

> Tier C is what makes the other two tiers honest. Without it, "is this
> compensation still needed?" is answered by argument.

`request_pipeline::sampling` was filed Tier B — Policy, permanently gglib's.
Two things were wrong with that. Its *floor* was Tier A misfiled, which
[ADR 0003](0003-defer-sampler-defaults-to-llama-cpp.md) settled by
measurement. And it had **no Tier C at all**. In two months the subsystem
produced roughly a dozen fixes and one outright reversal, every one of them
found by a person reading code rather than by an instrument.

ADR 0003 then made the gap load-bearing. It defers six sampler values to
llama.cpp on the strength of a single measurement against a pinned build, and
carries a deletion criterion that runs backwards from ADR 0002's:

> If a pin bump moves an upstream default that gglib now defers to, the
> readback flags the divergence and this decision is re-taken for that
> parameter.

Its decision 6 made the readback the precondition for the deferral, on the
grounds that deferring first would trade a known cost for an unmonitored risk.
This ADR is that readback: what it measures, what it refuses to measure, and
the one thing currently stopping half of it from working.

## Method

Two instruments, because "did the value arrive" and "what does the build
default to" are different questions with different failure modes.

**The slot comparison.** `slots_poller` has polled `GET /slots` every second
since #536 for the dashboard's context display, and `slots.rs` explicitly
discarded the one field that answers this question. That field is now parsed,
and each poll compares the `params` of every processing slot against the
`SamplingDecision`s in flight.

**The `/props` baseline.** `default_generation_settings.params` is the table a
request falls back to when gglib names nothing. Read once per model launch and
compared against the values ADR 0003 measured for the pinned build.

Everything below was measured on `b1-69bf643` against a real server, on
Llama-3.2-3B unless stated. The reproducer is
`scripts/experiments/sampler_wire_semantics.py`.

## Findings

### 1. The launch flags blind the `/props` instrument

The finding that shaped the design, and it inverts a dependency.

| field | build default | flag passed | `/props` reports |
|---|---|---|---|
| `temperature` | 0.8 | 0.7 | **0.7** |
| `top_p` | 0.95 | 0.90 | **0.90** |
| `top_k` | 40 | 33 | **33** |
| `repeat_penalty` | 1.0 | 1.07 | **1.07** |
| `presence_penalty` | 0.0 | 0.3 | **0.3** |
| `min_p` | 0.05 | 0.11 | **0.11** |
| `dry_multiplier` | 0.0 | 0.4 | **0.4** |

Every sampler launch flag overwrites the field it names, and gglib passes all
seven at values chosen to equal upstream's.

> **Amended 2026-08-09.** As first written this said gglib passes them "on
> every launch". That is wrong, and the correction matters because it changes
> where the instrument is blind.
>
> - **`gglib serve <model>`** — the primary path — resolves the *full* config
>   through the floor (`resolve_inference_config`) and hands it to the launch
>   as `ServerConfigOptions::inference_params`. All seven flags are passed, so
>   `/props` reports gglib's floor back to gglib. Blind.
> - **`gglib proxy`** standalone passes only `inference_override`, the user's
>   explicit CLI sampler flags, which is `None` unless they gave some
>   (`SamplingArgs::into_override`). On a default run **no sampler flags are
>   passed at all**, and `/props` would report the build's true defaults.
>
> So blindness is a property of *this launch*, not of the build, and
> `SAMPLER_LAUNCH_FLAGS_PASSED` is a compile-time constant modelling a runtime
> fact. It is wrong in the conservative direction — it claims blindness on a
> path that can actually see, never sight on one that cannot — which is the
> only direction decision 3 permits.
>
> **Resolved.** Deleting `to_cli_args` removed the flags from every path at
> once, so the constant is now unconditionally correct at `false` and the
> distinction between the two launch paths no longer affects what this
> instrument can conclude.
>
> Recorded rather than edited away because "the primary path is blind" and
> "every path is blind" support the same decision by different amounts, and the
> difference is exactly the sort of thing that gets re-derived later.

ADR 0003 finding 3 called those flags "inert twice over" and was right about
*request* behaviour: the body wins, so no model sees them. They are not inert
for *observation*. They overwrite the exact table the deletion criterion reads,
with values that match — so the check would report an agreement it could never
have failed to report. An organ reading its own reflection and calling it
health, which is [ADR 0002](0002-defer-tool-call-constraint-to-llama-cpp.md)
finding 2's inert-module trap in a new place.

So the dependency runs both ways. ADR 0003 decision 6 gated the deferral behind
the readback; this gates half the readback behind the flag deletion. Not a
deadlock — the slot comparison works today — but it means deleting the launch
flags is something the *instrument* needs, not merely redundancy worth removing.

**Recorded as a methodology note:** the first run of this measurement reported
"unmoved" on all six non-temperature fields, apparently refuting the
prediction. The flagged server had failed to bind an occupied port, so the
reading came from the bare server still holding it. Same shape as ADR 0002
finding 3 and as the determinism probe in ADR 0003's method section: a
comparison in which nothing could have varied, reporting that nothing varied.
Three for three now. A control that proves the apparatus can move is not
optional in this repository.

### 2. The slot comparison cannot see a resolution bug

Stated first because an earlier draft of this work claimed the opposite, and
the claim reached an accepted ADR before it was caught (ADR 0003 finding 7,
since amended in place).

#621 resolved `presence_penalty: 1.5` from the wrong layer and sent 1.5. #745
resolved `dry_multiplier: 0.0` after the coupling rule discarded 0.8, and sent
0.0. In both, intent and wire agreed perfectly. An intent-versus-wire
comparator reports nothing. gglib decided the wrong thing and transmitted it
faithfully.

The arc therefore has two halves and this is one of them:

| question | instrument |
|---|---|
| is what we resolved what the server got? | this ADR |
| is what we resolved what the user asked for? | `Displaced` provenance, property tests over the fold |

Neither substitutes for the other, and conflating them is how this organ's
purpose got overstated in the first place.

### 3. Coverage is a sample, biased toward long turns

`params` appears only on a slot that is actively processing, so coverage
depends on turn duration against the 1 Hz poll.

| turn duration | caught |
|---|---|
| ~5 s | 12/12 |
| ~0.6 s | 6/12 |

`comparisons` counts requests **observed**, never requests sent. No rate
derived from it is a rate over traffic.

The same fact has an upside: an idle slot reports *nothing* rather than the
previous request's values, so there are no stale readings to guard against.

### 4. Observations cannot be attributed, so the comparison abstains

gglib never sees llama-server's `id_task` in a chat-completions response, so a
slot cannot be joined to the request that filled it. Under any parallel client
several slots process at once.

Rather than invent a correlation, `compare_poll` compares only when every
intent in flight agrees on the compared fields, and otherwise counts
`skipped_ambiguous`. Measured cost:

| scenario | ambiguous polls |
|---|---|
| 4 concurrent turns, identical resolution (the default config) | **0 / 10** |
| 4 concurrent turns, parameters genuinely differing | **9 / 9** |

It abstains exactly where guessing would have been wrong, and essentially never
otherwise — because with `trust_client_sampling` off, every compared field
comes from the ladder rather than the client, so concurrent requests against
one model and profile resolve identically.

"Identical" means identical **on the compared fields**. Keying on the whole
`SamplingDecision` would abstain on nearly every poll, since `max_tokens` is
client-authoritative and varies per request while nothing else does.

### 5. `params` is an echo of the request, not the applied chain

Carried forward from ADR 0003 finding 7 because it bounds what a clean reading
means. Sending `mirostat: 2` alongside `top_k: 7` leaves `params` reporting
`top_k: 7` with a `samplers` array identical to a run without mirostat.

So a client's own unmodelled sampler can render gglib's values inert with no
divergence reported. **Absence of divergence is not proof the model sampled the
way gglib intended.**

### 6. The intents worth comparing are the ones in flight, not the recent ones

Not a measurement, but a design finding worth recording because the obvious
answer is wrong.

A ring buffer of recent decisions is the natural shape and it misbehaves: a
*finished* request whose parameters differed stays in the comparison set and
forces finding 4's abstention while nothing is actually ambiguous.

The correct set is exactly the set of requests in flight — which is precisely
the condition under which llama.cpp populates a slot's `params`. That set
already exists as `ActiveConnectionsRegistry`, already maintained by a `Drop`
impl covering completion, early return, client disconnect and panic. So the
intent rides `ConnectionGuard`, alongside the admission lease, for the reason
that module already gives for the lease: identical lifetime, guard already
travels it. Model-change invalidation falls out too — intents are asked for by
model name, so a swap needs no clearing step.

One trap inside it: filtering by `ConnectionPhase` to drop `Queued` entries
looks right and is unsound. Phase only advances on the streaming path's
progress frames, so a non-streaming request stays `Queued` for its whole life.
Filtering would hide the very intent an observation might belong to, converting
a correct abstention into a confident misattribution.

### 7. A model's own GGUF moves the table too, and finding 1 said only flags do

*Added 2026-08-09. Mechanism read from llama.cpp source at the pinned commit,
then measured — see ADR 0003 finding 2's amendment for the table. Stamping
`general.sampling.temp=0.33`, `top_k=17` and `min_p=0.011` into a copy of
`Llama-3.2-3B-Instruct-UD-Q6_K_XL` moved all three in `/props` on a bare
launch, and moved nothing else.*

Finding 1 named launch flags as what blinds `/props` and implied they were the
whole of it. They are one of three.

llama.cpp PR #17120 ("model-embedded sampling parameters", merged 2025-11-25;
the pin at `69bf643` is 3176 commits ahead of it, `behind: 0`) added
`common_init_sampler_from_model` in `common/common.cpp`, which overwrites
`params.sampling` from a model's own `general.sampling.*` metadata for every
field no CLI flag sets — tracked by a `user_sampling_config` bitmask, so the
precedence is **CLI flag > model metadata > build default**.
`tools/server/server-context.cpp` builds `default_generation_settings` from
that same struct.

Twelve keys exist. Five map onto a field this check compares:

```text
  gglib field        GGUF key
  temperature        general.sampling.temp
  top_p              general.sampling.top_p
  top_k              general.sampling.top_k
  min_p              general.sampling.min_p
  repeat_penalty     general.sampling.penalty_repeat

  presence_penalty   (no key — a model cannot move it)
  dry_multiplier     (no key — a model cannot move it)
```

The asymmetry at the bottom matters: the check cannot go fully blind on a
model's account, because two fields stay build-attributable whatever a model
ships. The other seven keys — `sequence`, `xtc_*`, `penalty_last_n`,
`mirostat*` — move sampling with nothing in gglib watching, since gglib has no
floor for them to contradict.

**The sting.** ADR 0003's flag deletion is what opened this instrument's eyes,
and the same pinned build had independently half-closed them. Worse, deleting
the flags is what *guarantees* model metadata wins, because a flag was the only
thing that would have suppressed it. Finding 1's conclusion — "blindness is a
property of this launch, not of the build" — was right and did not go far
enough: it is a property of this launch **and this model**.

## Decision

**1. The sampling boundary gets a Tier C organ, always on and never gated.**
`sampling_audit` (slot comparison) and `props` (baseline check). Neither is
behind a setting: an observation you have to remember to enable is one you find
out you needed after the fact.

**2. It never acts.** ADR 0001's static-arbitration rule, and the case is
stronger here than for dialects. Feeding a 1 Hz poll back into resolution would
make two identical requests decode differently depending on when a poll landed,
and it would poison the request recorder the rest of this architecture is built
to feed. A divergence is logged, counted and surfaced. Acting on it means a
person changing something between runs, with the evidence in hand.

**3. Blind is a state, not a count.** `AuditState` is a tagged union —
`NotYetObserved`, `Blind { reason }`, `Comparing { comparisons, divergences }` —
and every surface must render `Blind` differently from `Comparing {
divergences: 0 }`. This is `RuntimeCapabilities::unknown`'s discipline
generalised from a capability probe to an observation organ: unknown means
nobody knows, never "the feature is absent".

Pinned in three places so it cannot erode: the type, the serialization
(`a_blind_sampling_audit_serializes_differently_from_a_clean_one`), and the UI
(`ProxySamplingPanel`'s first two tests).

**4. Only a demonstrated comparison clears blindness.** A server that becomes
reachable again but is never caught mid-turn has proved nothing. Recovery is
evidenced, not inferred.

**5. Abstention is reported, and reported separately from blindness.**
`skipped_ambiguous` sits beside `AuditState`, not inside it. Abstaining is
something a *sighted* organ does; a large count next to few comparisons means
the traffic cannot be attributed, which is a different problem wanting a
different fix.

**6. A masked field is `Indeterminate`, never `Matches`.** Finding 1's
consequence, applied per field. `SAMPLER_LAUNCH_FLAGS_PASSED` records the state
and `flag_deletion_flips_the_switch` fails the build if it drifts from what
`to_cli_args` actually emits — in either direction, so the instrument can
neither claim sight it lacks nor stay blind after it is freed.

**7. The stated limits are part of the contract.** Findings 2, 3 and 5 are in
the module documentation, not only here. An instrument whose limits live in a
document nobody opens is one whose clean readings will be over-trusted.

**8. A model-supplied default is its own verdict.** Finding 7's consequence.
`ModelSupplied { key, value }` — never `Matches`, never `Differs`. It does not
fire the reverse deletion criterion (a model moving a value says nothing about
whether a pin bump moved the value gglib defers to) and it does not satisfy it
either (the build's own default is unobservable for that field), so it counts
against coverage rather than toward agreement.

*The alternative, recorded because it is the tempting one.* Compare `/props`
against `model_value ?? UPSTREAM_DEFAULTS` and report `Matches` when they
agree. That comparison **cannot fail**: the observed value *is* the model value
by construction, because llama.cpp wrote the model's number into the struct
`/props` renders. It would report health forever, including on a build whose
default had moved to something else entirely. That is finding 1's trap and ADR
0002 finding 2's inert-module trap for the fourth time in this subsystem, which
is roughly once per opportunity.

A model value that `/props` *contradicts* is `Indeterminate`, not `Differs`:
the attribution premise has failed and gglib cannot say which source won, so
blaming the build would be inventing a culprit. That arm doubles as a positive
control on the model-metadata path itself.

**9. Coverage is a property of the whole table, not of any field.**
`BaselineCoverage` replaces a `conclusive: bool` computed as "any field reached
a verdict" — under which a report covering two of seven fields rendered as "All
7 sampler defaults match the values this build was measured at". That is
decision 3's rule one level up: not a field claiming agreement it lacks, but a
*report* claiming completeness it lacks. Only `Complete` may render an
all-clear, and drift is checked before coverage so a partial reading can still
raise an alarm.

## Two rules that follow

### An observation organ has to be able to fail

Findings 1 and 4 are the same lesson at different scales. In both, the naive
implementation produces a confident answer that is structurally incapable of
being wrong: `/props` agreeing with the flags gglib set, and a comparison
attributing an observation to whichever intent came first. Both would have
looked like a working instrument indefinitely.

So the test for a Tier C organ is not "does it report health" but **"what would
have to be true for this to report a problem, and can that state actually
occur?"** Where the answer is no, the honest output is `Indeterminate` or
`Blind` — and those must be visibly different from health at every layer, or
the distinction dies at whichever layer collapses it.

### Measuring the boundary is not measuring the policy

This organ can now say that what gglib resolved is what llama-server received,
and that this build still defaults to what it was measured at. It says nothing
about whether gglib's sampling hierarchy improves agentic outcomes, whether the
temperature-coupling rule is sound, or whether `min_p` belongs in the coupled
trio.

Recorded because ADR 0003 made the same distinction about redundancy and it is
equally tempting here: a boundary you can see across is not a policy you have
evidence for. Those need an A/B instrument that does not exist yet.

## Consequences

**Good:**

- ADR 0003's deletion criterion has a mechanism for the first time, so its
  deferral can proceed under observation rather than on trust.
- A transmission fault — a value resolved and then lost to serialization or
  overwritten downstream — is now detectable in production rather than only by
  someone reading a `--verbose` log.
- "Why is gglib ignoring my temperature?" is answerable from the dashboard.
  `client_fields_discarded` counts the trust gate's work and says so.
- The `/slots` poll pays for itself twice, at no extra request cost and no new
  connection.
- `SlotParams` parses one shape for both `/slots` and `/props`, because
  llama.cpp hands back the same struct it initialises each slot from.

**Bad, and accepted:**

- ~~The baseline check concludes nothing until the launch flags are deleted. It
  ships blind and says so, which is better than shipping a check that cannot
  fail — but it is a real gap until the follow-up lands.~~

  *Resolved in this same arc, and struck rather than deleted because the shape
  of the gap is the instructive part.* The flags went with ADR 0003's deferral;
  the check went from `Indeterminate` on all seven fields to conclusive
  `Matches` on all seven, and `SAMPLER_LAUNCH_FLAGS_PASSED` is now `false`. See
  the follow-up below, which records the same thing from the other side.
- The build's own default is unobservable for any field a model supplies from
  its GGUF (finding 7), so baseline coverage is per-launch-and-model rather
  than per-build. A model declaring all five reachable keys leaves only
  `presence_penalty` and `dry_multiplier` under observation.
- Coverage is a sample biased toward long turns (finding 3), so a fault
  affecting only fast requests is systematically under-observed.
- A clean reading does not mean the model sampled as intended (finding 5).
- Every intent in flight is cloned once per poll. Trivial at 1 Hz against a
  handful of connections; worth remembering if either number grows.

**Neutral:**

- `gglib proxy dashboard` (CLI) consumes the same contract and does not render
  the readback yet. Its `serde(default)` mirror ignores the field harmlessly,
  but the two surfaces are not at parity.

## Follow-ups

- ~~Delete the sampler launch flags, which is what switches the baseline check
  on.~~ Done. Verified live: the check went from `Indeterminate` on all seven
  fields to conclusive `Matches` on all seven, and
  `SAMPLER_LAUNCH_FLAGS_PASSED` is now `false`. It was kept rather than deleted
  with the flags, because the failure it guards against is a flag *reappearing*
  — `no_sampler_flag_may_reappear_unnoticed` asserts both halves across the two
  crates, and `build_command` now calls `sampler_flags()` so that assertion is
  about the launch path rather than about a function nobody called.

  *Annotated 2026-08-09:* that verification was taken on a model carrying no
  `general.sampling.*`, which is why finding 7's gap survived it. "Conclusive
  `Matches` on all seven" was true of that model, not of every model.
- Render the readback in the CLI dashboard, for parity with the GUI.
- Have `model explain` print the build's default beside a deferred field, so a
  `—` reads as "llama.cpp chose 0.95" rather than as a gap. This is what ADR
  0003 decision 5 was after; the `ParamSource::Deferred` variant it proposed
  turned out not to be the mechanism (see the amendment there) — `Unset`
  already carries the distinction, and what is missing is the number, which
  comes from [`props`].
- Correlating a slot to a request is closer to hand than this ADR assumed.
  `to_json_non_oaicompat()` (`tools/server/server-task.cpp:368` at the pin)
  emits both `id_slot` **and** `"generation_settings"` — the complete
  per-request resolved sampling — and PR #12246 (merged 2025-03-23, also in the
  pin) attaches that object as `__verbose` on the OAI chat path *including the
  streaming first delta*. It is unreachable only because `task_params.verbose`
  is never parsed from a request body nor assigned anywhere in the pinned
  server; it is a declared field with no way to set it.

  So the upstream ask is "make `verbose` settable per request", which is a much
  smaller change than "add `id_task`" — and it would retire finding 4's
  abstention entirely, remove finding 3's sampling bias, and make the whole
  slot-poll comparison a census. Finding 4's abstention is the right answer
  today; it should be read as a workaround with a known exit rather than a
  permanent constraint.
- Read `general.sampling.*` at import into the *inference hierarchy*, not only
  into the baseline check. gglib currently writes its own `reasoning_profile()`
  recipe for `reasoning`-tagged models; a model author's published
  recommendation is better evidence than gglib's guess, and ADR 0003's own
  argument — do not restate what upstream already decides — applies to it.

[`props`]: https://github.com/mmogr/gglib/blob/main/crates/gglib-proxy/src/props.rs
