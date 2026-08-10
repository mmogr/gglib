# ADR 0004 — Observe the sampling boundary

- **Status:** Accepted
- **Date:** 2026-08-09 (amended 2026-08-09 — finding 1 overstated which
  launch paths pass sampler flags, and named launch flags as the only thing
  that masks `/props`; see the amendment there and finding 7. Addendum
  2026-08-09 — why there are no request-level task overlays, moved out of
  #744's PR body so the prohibition outlives it)
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

*That instrument now exists and has been run.* It does not change the paragraph
above — the organ still says nothing about policy — but the second addendum
records what the separate instrument measured, and what it does not settle.

One policy question *was* settled while this ADR was being written, by running
the code rather than by this organ. It is recorded in the addendum below,
because the decision it reached is a prohibition and prohibitions decay when
their reasons live only in a merged PR body.

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
- Re-run the agentic eval now that the A/A arm exists, so the second addendum's
  +0.082 can be quoted against a measured drift instead of against the raw arm's
  own instability. Until then that figure is a direction with a plausible
  magnitude, and the addendum says so.

## Addendum — request-level task overlays, and why there are none

*Added 2026-08-09. Records a decision taken across #741, #743 and #744 whose
reasons currently live in one doc comment and three merged PR bodies. It is
here because the thing it prohibits is a recurring, plausible-sounding
proposal, and because what refuted it was a measurement — the same pattern
finding 1 and the two rules above keep arriving at.*

### The proposal, and what exactly is banned

The recurring idea: have the proxy read a request, infer what task it is —
coding, tool emission, prose — and overlay a task-appropriate sampling recipe.
Usually phrased as *"drop the temperature when it's a coding prompt."*

gglib does not do this. **The ban is on the classifier, not on task-awareness**,
and the distinction is the whole of it — a mild, provenance-gated ceiling
*does* ship, and re-reading this section as "task-aware sampling was tried and
removed" would contradict live code in `request_pipeline::sampling`.

Three things are prohibited, and they compose:

1. Inferring the task from request *content* (prompt sniffing).
2. Driving the temperature to a task-appropriate low or near-greedy value on
   the strength of that inference.
3. Expressing either as a ladder rung.

### 1. The single-completion dilemma

The load-bearing argument, and the one that generalises past sampling.

A reasoning model does not decode its tool call or its code in isolation. The
`<think>` block and the payload are **one completion under one sampler
configuration**. There is no point between them at which a proxy could change
the temperature, because from the server's side there is no boundary — only a
token stream that eventually contains a closing tag. So a temperature chosen
for the code lands on the reasoning that precedes it.

Both vendors specify the range this breaks, and specify it for this reason:

| model | one completion contains | published guidance |
|---|---|---|
| Qwen3 (thinking) | `<think>` … `</think>` **and** the code or tool call | ~0.6; explicitly warns against greedy decoding |
| DeepSeek-R1 | the same | 0.5 – 0.7 |

Below that range these models degenerate into endless repetition — which the
proxy's own pre-dispatch loop guard (#723) then rejects as a 400. A near-greedy
overlay therefore does not merely sample poorly. It **manufactures the exact
failure another organ exists to catch**, and it does so on the models most
likely to be used for agentic coding, since `reasoning` tagging is automatic at
import for Qwen3.x, DeepSeek-R1 and QwQ.

Verified rather than reasoned: tool-carrying requests against Qwen3.5-4B
returned populated `reasoning_content`, confirming the two phases share one
completion in the traffic this would have applied to.

### 2. The classifier cannot know what it claims to know

Independent of the dilemma, and fatal on its own.

`carries_tools` answers *"could this turn emit a call?"* — never *"will it?"*
VS Code Copilot in agent mode sends `tools` on essentially every request, prose
included, so on that traffic the two questions have visibly different answers
and only one of them is askable.

This is the same wall `constrain.rs` hits under `tool_choice: "auto"`, where it
is documented and accepted: a grammar constrains from the first token, so
installing one would forbid the plain-text answers `auto` exists to permit —
and the stage therefore leaves `auto` unconstrained rather than guessing. Both
stages want to know something about the output before any of it exists. One of
them already concluded that you cannot.

Prompt sniffing is strictly worse: it adds a content heuristic that is wrong in
a way nobody can audit from a resolved value, in front of a decision that was
already unable to justify itself.

### 3. A ladder rung would have taken the coupled trio with it

The implementation trap, recorded because expressing "outranks the
auto-detected recipe" as a rung is the obvious way to build it.

A rung that names a `temperature` **claims the coupled trio** under
`resolve_layers`: `presence_penalty`, `repeat_penalty` and `min_p` then come
only from the layer that named the temperature. A `reasoning` model would
silently lose the `1.5` presence penalty its own recipe pairs with its
temperature, on every agentic turn. Clamping after the fold and gating on
`sources.temperature` leaves the trio untouched.

> **A correction worth keeping.** #744 stated this trap as costing "DRY and the
> penalties". DRY was coupled only between #741 and #746, which removed it on
> the measurement that DRY's strength is governed by its own `dry_base` and
> `dry_allowed_length`, and that it targets a failure mode which gets *worse*
> at low temperature rather than milder. The clamp is still the right shape;
> the reason is the trio, and a reader who checks the old wording against
> `CoupledLayers` will find three fields, not seven.

### What actually ships, and why it is not the banned thing

A temperature **ceiling** of `0.6` on `reasoning`-tagged models and `0.3`
otherwise ([`agentic_temperature_ceiling`]), clamped *after* the fold and gated
on provenance.

| | the banned overlay | the shipped ceiling |
|---|---|---|
| trigger | inferred from prompt content | `tools` present on the request |
| value | task-appropriate, near-greedy | `0.6` / `0.3` — inside vendor range, never greedy |
| overrides a person | yes | **no** — only the auto-detected rung and the floor |
| can raise a temperature | yes | never |
| mechanism | a ladder rung | clamp after the fold; the coupled trio is untouched |

An auto-detected recipe is an unreviewed guess written at import time, and
`DefaultsOrigin` already ranks it below global settings for that reason — so a
cap overruling it is consistent with the hierarchy rather than an exception to
it. Measured on Qwen3.5-4B: no tools → `1.0`; with tools → `0.6`; a profile
setting `0.9` with tools → `0.9`, DRY intact.

**A person may still ask for a near-greedy coding temperature.** That is what
`{model}:{profile}` is for — `gglib config profile set coding --temperature
0.15`, selected per request as part of the requested model name. The request
*declares* the task instead of the proxy guessing it, and the value is one
somebody chose and can be shown. The ban is on inference, not on the outcome.

### The methodological point

#741 shipped the floor. It was not reviewed into correctness — it was **run**,
and measured inert: a tools request against Qwen3.5-4B resolved identically to
a chat request, `temperature 1.0`, `top_p 0.95`, because every `reasoning`
model carries an auto-detected recipe and any layer outranks a floor. The
feature was doing nothing on precisely the models it was written for, and would
have gone on reporting success indefinitely.

This is [ADR 0002](0002-defer-tool-call-constraint-to-llama-cpp.md) finding 2's
inert-module trap once more — decision 8 above was already counting its
recurrences, and this is one it does not count, because it happened in policy
rather than in an instrument.

That is the part worth carrying forward. The two rules above were written about
Tier C organs, on the reasoning that an observation which cannot fail is not an
observation. #741 shows the rule is not confined to organs: **a sampling change
that has not been observed firing has not been shown to fire**, and a floor
beneath a layer that always names the same field can no more report its own
inertness than a `/props` read can report a flag it is echoing. Both were
answerable only by running them.

### What would reopen this

Not an argument — evidence from the A/B instrument the section above says this
ADR's organ cannot supply. Specifically: a run showing that a task-conditioned
temperature beats the ceiling on tool accuracy *without* degrading the same
model's reasoning phase, on multiple seeds.

Two cautions for whoever runs it. Single-sample scoring has already produced a
`0.728` vs `0.543` spread between raw arms that were configured identically, so
a difference smaller than that gap is not a result. And part of that spread was
not stochastic at all — a runaway tool-call stream was destroying whole turns
and scoring them `0`, on a different task in each arm, which made two identical
arms look like they diverged. A control that proves the apparatus can move,
again, is not optional here.

## Addendum — the A/B instrument, and the first thing it measured

The section *"Measuring the boundary is not measuring the policy"* ends by
saying the policy questions need an A/B instrument that does not exist. It was
built, and run. This records what it returned, and — at greater length, because
this is the part that decays — what that return does not license.

### What was run

`gglib benchmark agentic`, on Qwen3.5-4B Q8_0 at 131072 ctx: nine BFCL-style
tasks, three arms, five seeds each, 45 runs per arm and 135 in total. The arms
differ only in which request reached the same loaded llama-server — `raw`
bypasses the pipeline entirely, `gglib` carries it, and the third is the
control below.

| axis | raw | gglib | delta |
|------|----:|------:|------:|
| tool accuracy | 0.867 | 0.944 | +0.078 |
| task completion | 0.822 | 0.911 | +0.089 |
| loop avoidance (eligible) | 0.250 (12) | 0.333 (15) | +0.083 |
| composite | 0.651 | 0.733 | **+0.082** |

Every axis moved the same way. The largest single contributor is not a coin
landing twice: `multi_turn_search_then_read` failed on **all five** raw seeds
and passed on two gglib seeds, which is a categorical difference rather than a
shift in a rate.

### The control failed first, for a reason this ADR had already recorded

The first control forced `temperature: 2.0` and nothing else. It scored **above
both real arms** — a result that reads as absurd until it is read against
[ADR 0003](0003-defer-sampler-defaults-to-llama-cpp.md) finding 5, which
measured that llama.cpp applies the truncation samplers *before* temperature.
With a `reasoning` recipe's `top_k: 20` still in force, temperature 2.0 was
flattening a distribution over twenty surviving tokens. The number looked
extreme; the change was not.

The fix was to disable `top_k`, `top_p` and `min_p` outright. The control then
scored 0.237 against the pipeline's 0.733 — a **0.496** gap, with
`loop_eligible: 0`, meaning not one of 45 runs reached a second tool-call batch.

Two things follow, and the second is the one worth keeping.

1. The apparatus can detect a sampling change, so a null result elsewhere in
   the report would have been evidence rather than silence.
2. **A control is a piece of code and can be wrong in the same way a feature
   can.** A control that fails to degrade reports the same "no difference" as a
   harness that cannot see, and this one was wrong for a reason already written
   down in a sibling ADR. It was caught only because
   [`ControlVerdict`][control-verdict] distinguishes *moved the wrong way* from
   *barely moved* — the earlier boolean rendered a 0.090 wrong-direction swing
   as "changed by only −0.090", which reads as *barely moved* about a control
   that moved a great deal, in the wrong direction. That is decision 3's rule
   (a state that licenses a different action must render differently) applied
   to a verdict rather than to a field.

### What the +0.082 does not establish

The control validates sensitivity **at 0.496, not at 0.082**. It shows the
apparatus responds to a large change. It says nothing about whether the
apparatus can resolve this one, and treating it as though it did is the most
available misreading of the whole report.

The measured effect is **+4 task-seed passes out of 45**. Within the same run,
the raw arm disagreed with *itself* across seeds on three tasks. An effect of 4
against a within-arm instability of ~3 is not separable by inspection.

So a second calibration arm was added afterwards: `raw_replicate`, the raw arm
re-run on a **disjoint** seed set with nothing else changed — an A/A test. The
gap it opens is the eval's own drift, and [`EffectVerdict`][effect-verdict]
compares the headline delta against it. The seeds have to differ: replaying the
same ones would measure whether a fixed seed replays, which is not what limits
the comparison — *which* five seeds were drawn is.

That arm postdates the run above, so **this ADR quotes no drift figure**. The
+0.082 is recorded here as a direction with a plausible magnitude, not as a
resolved magnitude, and the next run of this eval is the one that can say which.

One caution carried forward from the design: a single A/A pair estimates drift
from one degree of freedom. It is enough to stop a delta *inside* its own noise
being reported as a finding. It is not a significance test, and the honest way
to strengthen it is more pairs rather than a larger threshold.

### The cost finding

The fixed control took **161 of the run's 174 wall-clock minutes** and generated
254,688 completion tokens against the pipeline's 47,116 — roughly 15× the arm it
validates, because broken sampling makes the model ramble until a guard stops
it. The seed count was never what made the eval expensive; the control was.

It has since been cut to one seed by default. It can afford the imprecision:
the two real arms are compared against *each other* and need every seed, while
the control only has to clear a threshold an order of magnitude below the gap it
opens. Both surfaces now print its sample size beside its composite, because a
one-seed number sitting in a column of five-seed numbers otherwise reads as one
of them.

### What this changes about the ban above

Nothing. The first addendum bans request-level task overlays and ships a live
temperature ceiling instead; this measurement was taken *with* that ceiling in
force and does not bear on the comparison the ban is about. The reopening
criterion in *"What would reopen this"* still stands, with one of its two
cautions now satisfied: a control that proves the apparatus can move exists, and
works. The other — that a difference smaller than the raw arms' own spread is
not a result — is exactly what the A/A arm was added to adjudicate, and it is
the criterion this run cannot yet meet on its own.

[control-verdict]: https://github.com/mmogr/gglib/blob/main/crates/gglib-core/src/domain/benchmark/agentic.rs
[effect-verdict]: https://github.com/mmogr/gglib/blob/main/crates/gglib-core/src/domain/benchmark/agentic.rs
[`agentic_temperature_ceiling`]: https://github.com/mmogr/gglib/blob/main/crates/gglib-core/src/domain/inference.rs
[`props`]: https://github.com/mmogr/gglib/blob/main/crates/gglib-proxy/src/props.rs
