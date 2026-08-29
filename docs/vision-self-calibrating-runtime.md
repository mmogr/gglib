# The self-calibrating runtime

- **Status:** Vision and roadmap. Deliberately **not** an ADR — nothing below
  is decided until its phase lands with its own ADR and evidence. This is the
  document that keeps the destination legible between sessions.
- **Date:** 2026-08-29
- **Provenance:** distilled from the eval-repair arc (PRs #957–#959, merged)
  and the tool-call runaway investigation (PR #961). Every number below names
  its source; readings that cannot be re-derived are flagged in their own
  section at the end, per CONTRIBUTING's handoff-brief rules.

## The vision, in one line

**gglib is the runtime that learns your models by watching them work.** Every
other local-inference tool is configured once and then static; gglib sits in
the conversation path, keeps evidence per model, and adjusts its own posture —
interpose, assist, or get out of the way — on categorical evidence it can show
the user.

The product shape that follows: a model works the moment it is imported, in a
conservative posture. Evidence accumulates from real traffic. When gglib
changes how it treats a model, it announces the change with its evidence and a
one-command revert. The brand is the honesty: *the runtime that shows its
work — and will tell you when to turn it off.*

## Why this and not something else

The 2026-08-28/29 arc established four things, each with a receipt:

1. **The interposition machinery can hurt.** The originated tool-call grammar
   was unbounded (`root ::= sp call (sp call)* sp`); on
   `long_context_planted_values`, Qwen3-4B emitted **606 tool calls in one
   response** (`kept=64 dropped=542` in `logs/daemon.log`, 2026-08-29; an
   earlier run logged `dropped=1237`), stopping only at the context limit:
   853.9s and 32,836 tokens against 6.4s and 511 raw, both scoring 1.0
   (`one_task.json`). Fixed by bounding the grammar (PR #961, open at time of writing); verified
   bound-50 → 84.8s, and a diagnostic bound-1 probe → 3.5s with all four
   argument values correct.
2. **The interposition machinery also fixes what nothing else can.**
   `single_call_verbatim_payload` (present in `tune_default_suite.json`) failed
   in the raw arm and passed in the gglib arm across the two full-suite runs —
   *transcript-sourced; `run.json` was overwritten and the figure is no longer
   re-derivable, see the last section.* The reasoning under it is design, not
   measurement: llama-server is one-shot, and a retry is a conversation-level
   operation a stateless server cannot own, so validate-and-reissue is a niche
   only this layer can occupy.
3. **On a well-supported model, most of the machinery is neutral, and the
   suite is saturated.** The 21-task suite (`tune_default_suite.json`) left the
   large majority of tasks tied between arms in both full runs, and 19 of 21
   passed even in the arm whose sampling was *deliberately broken* — the
   control that is supposed to fail. Both figures are **transcript-sourced and
   not re-derivable** (see the last section); they are recorded as the
   motivation for the discriminative-power analysis in Phase 2, not as
   established magnitudes. gglib's value concentrates in failure classes and in
   models llama.cpp handles badly — and the latter population has never been
   measured at all.
4. **The instrument now tells the truth.** Crash dilution, the scorer race,
   mixed-population arithmetic, and the missing generation-shape observability
   were all fixed and proven able to fail (#957–#961). The observability is
   what found the runaway.

Together: gglib's durable job is the **conversation-level layer** (repair,
retries, guards, observability, routing); its decode-time constraints are
**scaffolding with deletion criteria**. Which posture is right is a per-model,
per-build, empirical question — ADR 0002 already learned that a deferral
measured on one model is overturned by the next (CONTRIBUTING's ADR section
records that retraction, and why it was kept rather than deleted). So the differentiating asset
is not any single mechanism: it is the ability to **measure honestly and act
on the measurement automatically**.

## The posture model

Per model (× quant × llama.cpp build), gglib holds one of three postures:

| posture | what it means |
| --- | --- |
| **native** | llama.cpp's own template machinery; gglib observes only |
| **native + net** | native handling, plus validate-and-reissue and guards |
| **full interposition** | gglib's dialect grammar, parsing, repair, bounds |

The posture is chosen by evidence, never by assumption; defaults are
conservative (**native + net** — correct everywhere, optimal nowhere); every
automatic change is announced with its evidence and is revertible in one
command.

## The two standing objections, answered by construction

**"Measuring every model takes too long."** There is no blocking exam. The
model is usable at import in the conservative posture. A background screen of
the few *discriminating* tasks takes minutes and sets the prior — the suite is
saturated enough that most of it carries no signal, which is what Phase 2's
analysis exists to quantify. After that, the measurement
is the traffic itself — repairs fired, runaways caught, guards tripped —
collected at zero marginal cost on the user's real workload. Fingerprints are
cached per model × quant × build, and are shareable in principle.

**"It is a big assumption the measurement is right."** Nothing is ever bet on
a measurement being right. The instrument distrusts itself (A/A drift arm,
positive control, withheld deltas); posture changes key only on **categorical**
findings, never small deltas; defaults are safe; and the live sensors audit
every decision forever, so a wrong fingerprint is contradicted by production
and rolled back. This is ADR 0006's *recover, don't predict* applied to
gglib's own judgment.

## The buildout

Each phase is independently shippable, lands with its own ADR and kill
criteria where it adds mechanism, and leaves no scaffolding behind.

### Phase 0 — close the current arc

- Merge #961; run the full suite on the corrected instrument (first
  trustworthy end-to-end baseline).
- **The thesis test:** the same suite on a model llama.cpp handles badly. The
  single most informative run the project can do — the target population has
  never been measured.
- Bound from configuration (`max_parallel_tools`, and the client's
  `parallel_tool_calls` field) instead of the hard ceiling; then delete the
  `GGLIB_MAX_GRAMMAR_TOOL_CALLS` diagnostic override.
- Chase the `max_tokens`-never-reaches-the-wire gap; equalise the eval arms'
  decode parameters so future deltas attribute cleanly.
- Root-cause the runaway mechanism (leading open question: what
  `tool_choice: "none"` beside a custom grammar does to tool rendering in the
  prompt). The bound is containment, not necessarily the fix.

### Phase 1 — durable evidence (the keystone)

The proxy's per-model defect counters are per-process and reset on restart
(recorded in ADR 0010/0011's first readings). A runtime cannot learn from
evidence it forgets. Persist a per-model ledger (repairs, runaways, guard
trips, latency shape, keyed by model × quant × build), surface it in
`gglib model explain` and the GUI, and write the telemetry-contract ADR.
Everything later is a consumer of this table.

### Phase 2 — the fingerprint

The deferred discriminative-power analysis picks the minutes-long mini-suite
(and addresses suite saturation in the same stroke). A background exam on
import stores the result on the model row with provenance, exactly as
`DefaultsOrigin` already works.

### Phase 3 — routing

The posture field, driven by ledger + fingerprint, categorical evidence only,
announce-with-evidence, one-command revert. Deletion criterion attached at
birth: an interposition no model family ever needs is dead weight and goes.

### Phase 4 — the mid-stream circuit breaker

The generation-shape sensors (PR #961) move into the live path: Nth identical
call in one response with no stop in sight → cut, tighten, re-issue. Ships
only with control-style proof: it catches seeded runaways **and never fires on
clean runs** — a false positive here destroys real user generations.

### Phase 5 — proof and reach

The two-week daily-driver trial (the loop audit's standing recommendation);
the measured headline ("small model + gglib vs a larger model raw"); shared
fingerprints last, once the local loop is trustworthy.

## What is deliberately not built

Community infrastructure before Phase 5; dashboards beyond GUI parity; new
suite tasks beyond what the discriminative analysis demands; speculative
model-family support. Each is real work with no customer until the loop
closes.

## Readings that cannot be re-derived

Per the handoff-brief rules, these were measured in-session on 2026-08-29 and
their artifacts were not kept; re-run before citing them further:

- The parallel-call check (`parallel_call_two_cities`, bound 50): the model
  emitted exactly 2 calls, not 50 — evidence the bound does not defeat healthy
  parallelism — at 3.0s/235 tokens vs raw 4.9s/399. Reproduce with a one-task
  suite extracted from `tune_default_suite.json` and
  `--seeds 12345 --no-control --no-replicate`.
- The bound-1 probe of the same task (llm_calls 3, still 1.0, ~29% slower than
  bound 50) — the measurement behind "do not default the bound to 1".
- `one_task.json`, `bounded50.json`, `bounded1.json` exist untracked in the
  repo root on the machine that ran them; the commands that produced them are
  in `docs/benchmark/README.md`. These three **were** re-verified on
  2026-08-29 and their figures above match them exactly.
- **`run.json` no longer exists.** Both full-suite runs wrote to that one path
  and the second overwrote the first; nothing on disk now carries either. So
  every full-suite figure in this document — the tie counts, the 19-of-21
  saturation reading, the `verbatim_payload` result — survives only as terminal
  output in a session transcript. Treat them as motivation, never as evidence,
  until Phase 0's baseline run reproduces them to a file. Future eval runs
  should write to a dated filename for exactly this reason.
