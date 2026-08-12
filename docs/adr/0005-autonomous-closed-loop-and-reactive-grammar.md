# ADR 0005 — The autonomous closed loop, and reactive grammar repair as the permanent mechanism

- **Status:** Superseded
- **Date:** 2026-08-12
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md),
  [ADR 0002](0002-defer-tool-call-constraint-to-llama-cpp.md),
  [ADR 0003](0003-defer-sampler-defaults-to-llama-cpp.md),
  [ADR 0004](0004-observe-the-sampling-boundary.md)
- **Supersedes:** nothing
- **Superseded by:** [ADR 0006](0006-recover-dont-predict.md)

> **Superseded by [ADR 0006](0006-recover-dont-predict.md).** The scheduler
> described here — the organ that decides when to spend the GPU — has been
> removed. The tuner kept rediscovering the values models already publish, a
> 27B tune costs 2–3 hours, and the failures people actually felt were never
> repaired by it. The gate, the ledger and the measurement suite all survive;
> the autonomy does not. Read 0006 first: everything below about the scheduler
> is history.

## Context

ADRs 0002–0004 built the instruments: a measured boundary, an A/B eval with
a positive control and an A/A noise floor, and the discipline that a
sampling change which has not been observed firing has not been shown to
fire. This ADR records what was built *on* them — the closed measurement
loop shipped across #779–#786 — and the one mechanism that was probed and
deliberately not built.

Every decision here was ratified by the project lead on 2026-08-12, and
every load-bearing mechanism was verified live on real hardware before this
document was written. Two of those verifications found and fixed real
defects (#779's gated-key body passthrough; the unseeded calibration twins),
which is the order of operations this ADR exists to preserve: the loop's
rules are codified failures, not principles argued into place.

## The architecture

Three organs, one loop: the ledger observes, the scheduler decides when to
spend the GPU, and the gate decides whether anything changes.

**The defect ledger** (`gglib_core::domain::defects::ModelDefectLedger`) is
Tier C made per-model: cumulative counts of requests, loop-guard trips, and
tool-call repairs, written by the proxy through the context-metrics store
(which already sees every event with the model name attached — the wiring
is one constructor call, zero call-site changes). Supervisor-owned so it
survives proxy restarts; deliberately **unpersisted**, because a defect
rate is a claim about recent traffic on this build of everything, and
yesterday's rate answering today's question is the staleness ADR 0001
warns about. Windowing belongs to readers: the scheduler keeps its own
baselines and rates deltas, so acting on a signal advances the baseline
and one burst of events can never fire twice.

**The idle-time scheduler** (`benchmark::auto_tune`, in the daemon — the
one process that owns llama-server) runs nothing unless `Settings.auto_tune`
is deliberately on: autonomy that spends hardware is opt-in, the opposite
polarity of the endpoint guards beside it. Its rules, each with its own
test: idle means idle (zero in-flight *and* zero waiters, sustained);
**a warm model is never evicted** — a resident untuned model is tuned in
place, a resident tuned model stands the scheduler down, because handing
the next real request a cold prefill is a price an idle-time nicety may
not charge; **a person's work is never the target**; any waiting request
preempts the run outright; and one attempt per model per seven days,
because a refusal is an answer and answers do not expire in an afternoon.
Signal-driven runs (a loop-trip rate ≥ 5% over ≥ 50 windowed requests →
a DRY sweep) bypass exactly two rules — the untuned-only filter and the
interval, both because a production defect is *new evidence* — and honour
the rest.

**The `Measured` defaults origin** is where an applied winner lives: below
global settings, like `AutoDetected` and `Published`, on the ladder's
oldest principle — nothing a person chose may be outranked by anything a
person did not, and an automated apply is not a person. It differs from
its rung-mates twice: the agentic-turn ceiling never caps a measured
temperature (the sweep resolved its candidates against the model's real
context precisely so the winner transfers — capping it would un-measure
it, #748's transfer bug one layer up), and every apply records the
displaced defaults on the run row, so any change is reversible without
archaeology.

## The drift-gated apply

An auto-tuner without a gate optimises noise: it ratchets whichever
candidate a lucky draw favoured into the catalog and reports improvement
while doing it. Both pre-existing apply paths (`--apply-best`, the GUI
checkbox) did exactly that, ungated. The gate replaces them with a rule
per measured failure:

- Every tune run carries an **incumbent calibration pair** — the all-`None`
  overlay (exactly what the model does today) run twice. The twins run
  **seeded**: every candidate sees task *T* under the same derived seed
  (common random numbers keep candidate comparisons paired), and the twin
  alone runs offset seeds, so its gap from the incumbent samples genuine
  seed-to-seed variance. Unseeded, the first live run measured the twins
  identical — drift 0.000, a calibration instrument reading zero because
  nothing was pinned — and at zero drift the ratio gate passes any margin.
- A winner applies only when its margin over the incumbent mean clears
  `EFFECT_NOISE_RATIO` × the twins' gap (the +0.082 that did not
  replicate), its per-task pairs do not vote against its mean (the
  lucky-outlier shape), and no compared candidate carries unmeasured runs
  (the arm of 45 zeros that rendered as a score).
- **Refusals are first-class verdicts**, never errors, each naming the
  evidence that was missing or contrary — including `IncumbentStands`,
  a successful run whose answer is "change nothing". The loop's first
  fully unattended act was exactly that refusal (run #40, incumbent
  0.664), and its first seeded manual run declined to churn its own
  previously applied recipe (`PairedDisagrees`, 0W–1L). An autonomy whose
  most common act is a documented refusal is the design working.

## Proactive lazy grammars: probed, parked, and what ships instead

The prospective final organ — engaging a decode-time grammar from turn one
on models whose repair rate shows their `tool_choice: "auto"` path is
unconstrained — was **probed before being built**, per the house rule, with
a canary grammar whose engagement is visible in the bytes.

The probe's verdict: **the mechanism is sound and the endpoint blocks it.**
Via `/completion`, `grammar_lazy` + a preserved trigger token behaves
exactly as documented — prose flows free, and generation snaps to the
grammar the moment the model emits the trigger. Via `/v1/chat/completions`
— the endpoint gglib forwards to — the chat-template layer **overwrites
`grammar_lazy`, `grammar_triggers` and `preserved_tokens` unconditionally**
with the template's output (`tools/server/server-common.cpp`; the guard
that protects `grammar` itself does not cover its three companions), so a
request-level grammar runs eagerly from the first token: the probe's prose
arm was force-fitted into a tool call and reasoning blocks were suppressed
outright. That is ADR 0002's wall, measured rather than only documented.
Two contract findings worth keeping: the trigger `type` is an integer enum
(a string fails silently into eager behaviour and a 200), and a
single-special-token WORD trigger must appear in `preserved_tokens`.

**Decision: the reactive repair loop is the permanent shipping mechanism.**
No llama.cpp fork will be carried for the three-line guard, and no
upstream issue will be filed; the probe script
(`scripts/experiments/lazy_grammar_auto_probe.py`) is the durable record
of the evidence. The repair loop's economics justify the choice: it costs
one extra generation on a failed call and nothing on a conformant one
(ADR 0002 finding 6, verified live), and the #786 ledger already counts
per-model repair rates — so if this decision is ever reopened, the signal
the feature would consume is already flowing.

**What would reopen it:** upstream guarding those three fields the way
`grammar` already is guarded, arriving through an ordinary pin bump — at
which point the probe reruns as-is, and the reasoning-block coexistence
question (unanswerable while the eager grammar crushed the think blocks)
gets its first real measurement.

## Consequences

**Good:** the loop acts alone and changes almost nothing — every write is
gated, logged with its numbers, reversible from the run row, and
subordinate to anything a person configured. The system's observability
carries its own lesson: every scheduler guard says why it stood down,
because the first live smoke test spent forty minutes being correct
invisibly.

**Costs, accepted:** two extra candidates per tune run (the calibration
pair); a scheduler that frequently declines to act, by design; and on
models whose templates install no native grammar under `auto`, first-call
conformance stays reactive — one failed call pays for the repair — until
upstream moves.
