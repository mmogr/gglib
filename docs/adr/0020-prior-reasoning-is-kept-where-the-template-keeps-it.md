# ADR 0020 — Prior reasoning is kept where the template keeps it: the request pipeline strips earlier assistant turns' reasoning only when the model's template is not known to preserve it

- **Status:** Accepted
- **Date:** 2026-10-14
- **Depends on:** [ADR 0007](0007-ask-the-server-for-template-capabilities.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** [log-0020](log-0020.md)

> **Amended 2026-10-15.** llama-server binds `preserve_reasoning` true by
> default only from `b11429`. Qwen3.8's template keeps earlier reasoning when
> it is unset, so for that model the Context holds at `b10327` too
> ([log-0020](log-0020.md)).

## Context

Stage 1 of the request pipeline removed `reasoning_content` and inline
`<think>…</think>` blocks from every earlier assistant message, for every
model. The reason written beside it was that small reasoning models such as
Qwen3.5-4B copy their earlier traces and never stop thinking.

llama-server probes each model's chat template at launch and reports what it
found as `chat_template_caps` on `GET /props`. gglib records that report on
the model's catalog row ([ADR 0007](0007-ask-the-server-for-template-capabilities.md)).
One of its fields, `supports_preserve_reasoning`, says whether the template
renders reasoning back for the whole history rather than only the latest
turn.

### What the template and the client do

- Qwen3.8's chat template, read from its GGUF on 2026-10-14, renders earlier
  assistant turns' `reasoning_content` back into the prompt when
  `preserve_thinking` is true (llama-server binds it true by default,
  `common/arg.cpp`, `preserve_reasoning`). The model's card calls preserved
  thinking "especially beneficial for agent scenarios".
- Copilot CLI 1.0.75 sent `reasoning_content` back on each assistant message
  in the requests logged that day.
- gglib stripped it, so the model re-derived its reasoning on each turn it was sent.

### One task, measured

One planning task, Qwen3.8-27B, thinking on, sent straight to llama-server.
Two runs with the earlier reasoning kept, three with it stripped. The runs
are not in this repository; the numbers here are the record.

| | Kept | Stripped |
|---|---|---|
| Reasoning on the longest turn | about 1,000 characters (1,136 and 898, the plan-writing turn) | about 16,000 characters (16,452 on the plan-writing turn; 15,822 and 16,612 longest turns, not scored as the plan turn) |
| Wall time | 11 min 22 s, 19 min 59 s | 15 min 15 s, 20 min 24 s, 24 min 15 s |
| Plan, graded against the merged fix | both at the top level, one of them the merged fix exactly | one at the top level; two lost or weakened the error rule (one run per condition, not attributed to the strip) |

On its own the wall-time shift is modest. Through the proxy, where the
reasoning was always stripped, the plan-writing turn re-thought 12,000 to
26,000 characters across six thinking-on runs, and it is re-thought again each
time the client asks for the plan again (a lap); one such run spent 54 of its
82 minutes in laps. That keeping the reasoning would shorten those laps is the
expectation, not a measurement.

The scope is one task, one model, two and three runs per arm. On this task
two runs of one arm can differ by about two to one.

### The two models the caps bit separates

Qwen3.5-4B's template has no `preserve_thinking` variable and renders
reasoning only for the current run
(`crates/gglib-gguf/src/capabilities/testdata/loop_guard_note/Qwen3.5-4B.jinja`).
Qwen3.8's template renders earlier turns' reasoning under llama-server's
default (see above), so the bit separates the two models: the one the strip
was written for, and the one measured above. Read from the templates, and for
Qwen3.8-27B from the caps too: a launch on 2026-10-14 recorded
`supports_preserve_reasoning: true` in its `template_caps` row. Qwen3.5-4B's
row has not been read (kill criterion 1).

## Decision

1. **Stage 1 strips prior reasoning only when the template is not known to
   preserve it.** When the model's recorded caps say
   `supports_preserve_reasoning: true`, every assistant message's
   `reasoning_content` and inline `<think>` blocks go to llama-server as the
   client sent them, in the current run and in earlier ones. When they say
   `false`, when they do not carry the field, or when the row has no caps
   (the model was never launched, or its build reported none), stage 1
   strips as before.

2. **The caps are the catalog row's**, the same ones the
   [ADR 0007](0007-ask-the-server-for-template-capabilities.md) effort gate
   reads through `ModelContext.template_caps`. The proxy's own `/props` read
   feeds its dashboard, not the pipeline.

3. **Every caller of the pipeline shares the rule.** A caller that resolves
   its model from the catalog gets the gate when the row carries caps: the
   proxy, the built-in agent on this machine (the web chat and `gglib chat`
   against a local target), the chat title, the tuner and the agentic eval's
   gglib and control arms (so an eval or tune result on a model whose row says
   `true` is not comparable with one taken before this ADR). A caller with no
   catalog row (a model on a paired machine, `gglib chat` against a remote
   target) has no caps and keeps stripping.

### Where it lives

- [`request_pipeline/messages.rs`](../../crates/gglib-core/src/request_pipeline/messages.rs):
  `strip_prior_reasoning`, which skips the strip on a positive answer.
- [`domain/template_caps.rs`](../../crates/gglib-core/src/domain/template_caps.rs):
  `preserve_reasoning_support`, the three-way answer.
- [`normalize/history.rs`](../../crates/gglib-core/src/normalize/history.rs):
  the strip itself, unchanged.

## Consequences

- **A preserve-capable model is sent its earlier reasoning.** On the task
  measured, that turned a re-derivation of 12,000 to 26,000 characters into
  one of about 1,000, at equal plan quality.
- **The prompt is longer by the reasoning kept**, and history truncation
  (stage 3) measures it with the rest.
- **When the template keeps reasoning, stage 1 leaves the messages
  untouched**: the history reaches llama-server as the client sent it, subject
  to the later stages (coalescing for a strict-turns model, truncation, the
  sampling keys). On a strict-turns model stage 2 merges consecutive assistant
  turns: their inline `<think>` text is concatenated with the content and only
  one `reasoning_content` survives, the first present; that is stage 2's
  existing rule and is not changed here.
- **The gate is a correlate of the looping failure, not its cause.** It
  separates the two models seen. If a preserve-capable model is ever seen to
  loop on its kept reasoning, the fallback is a repeat detector, written with
  that model in hand.

## Kill criteria

1. **The bit does not separate the models.** Reading: the
   `supports_preserve_reasoning` in the `template_caps` column of `models`
   for Qwen3.8-27B after a launch. Anything but `true` means this decision
   does nothing for the model it was written for. Reading so far: Qwen3.8-27B
   recorded `true` on 2026-10-14; Qwen3.5-4B not read.
2. **A preserve-capable model loops on its kept reasoning.** Reading: a model
   whose row records `supports_preserve_reasoning: true`, through the proxy,
   with an earlier trace in the history, thinks without closing until the
   length limit. At this commit the proxy records no per-request reasoning
   length or finish reason (PR #1424 adds both); the per-model
   `truncated_generations` counter and the dashboard's "truncated at ceiling"
   row are a partial signal, and the rest is seen at the client. One such
   model reopens decision 1 for it. Reading so far: the two kept runs in
   Context finished; no other reading taken.
3. **Keeping is slower or worse.** Reading: a graded rerun of the task in
   Context, more runs per arm, timed by wall clock. Kept runs slower than
   stripped beyond the two-to-one spread seen between runs of one arm, or a
   kept plan graded worse than a stripped one, reopens decision 1. The
   thresholds are this ADR's choice; nothing measured them. Reading so far:
   two kept and three stripped runs (Context).

## Out of scope

- Any detector for a model that repeats its own reasoning.
- The response side: how reasoning in a reply is parsed or shown.
- `reasoning_effort` and the reasoning budget, which keep their own rules.
