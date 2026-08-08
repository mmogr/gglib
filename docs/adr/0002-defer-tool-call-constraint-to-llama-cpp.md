# ADR 0002 — Defer tool-call argument constraint to llama.cpp

- **Status:** Accepted
- **Date:** 2026-08-08 (amended 2026-08-09 with a second dialect family)
- **Depends on:** [ADR 0001](0001-runtime-capability-tiers.md)
- **Supersedes:** nothing

## Context

[ADR 0001](0001-runtime-capability-tiers.md) classified `request_pipeline::constrain`
as Tier A — compensation, designed to be deleted — and set its deletion
criterion as *upstream constrains dialect tool calls under both
`tool_choice: "required"` and `"auto"`, with arguments conforming to the tool's
own JSON Schema rather than merely being well-formed JSON*.

It also set the rule that made this ADR necessary: **capability presence is not
permission to defer.** `RuntimeFlags::PEG_NATIVE_TOOL_CALLS` answers "does the
runtime attempt this?", not "should gglib stop attempting it?". The second
question is settled by measurement.

This is the measurement.

The planned work it scopes was the "schema-true tool calls" feature: derive a
GBNF grammar from each tool's `parameters` schema so that a call violating the
schema becomes unrepresentable at decode time, alongside post-hoc validation
and a constrained re-decode on violation.

## Method

`scripts/experiments/lazy_grammar_conformance.py`, run against **raw
llama-server with gglib out of the request path** — gglib's own normalization
and grammar origination would otherwise confound every reading.

| | |
|---|---|
| Build | `69bf643`, released as `b10327` — the pin |
| Model | `Qwen3.5-4B-UD-Q6_K_XL` (unsloth Qwen3.5-4B-MTP-GGUF) |
| Launch | `--jinja`, `-c 8192`, `-ngl 99` |
| Sampling | temperature 1.0, `max_tokens` 1024 |
| Volume | 6 adversarial prompts × 5 samples × 2 arms = 60 requests |

Conformant output under ordinary prompting proves nothing: a capable model
emits conformant arguments unaided. Three things separate *enforcement* from
*luck*, and the harness does all three — prompts written to tempt one specific
violation each ("read file number 42" invites `path: 42`), sampling at
temperature 1.0 so the distribution is exercised rather than one greedy draw,
and separate `auto` / `required` arms because llama.cpp treats them differently
(lazy trigger vs eager grammar).

## Findings

### 1. Upstream enforces the schema. 60/60.

```
  arm            calls   conformant     rate
  auto              30           30     100%
  required          30           30     100%
```

Zero type violations, zero enum violations, zero invented keys against
`additionalProperties: false`, zero missing required fields, zero non-call
outcomes. Every draw, under prompts designed to break each constraint.

`constrain.rs` by comparison constrains only the envelope, the function name,
and JSON well-formedness — it admits `{"path": 42}` against a schema demanding
a string. Upstream does not merely match this stage; it exceeds it. The
deletion criterion is met on the constraint dimension.

### 2. gglib's dialect parser is bypassed entirely.

Across all 60 requests, no raw markup reached the client. llama-server parses
the dialect itself and returns structured `tool_calls`.

`normalize::parsers::delimited` only fires on raw markup arriving in the text
or reasoning channel. For this model and this build it **never runs**. Not
because it is correct or incorrect, but because upstream gets first refusal on
the bytes.

This is the finding with the longest reach, and it was not anticipated by
ADR 0001. A Tier A module can become inert without anyone deciding to retire
it — and while inert, it is neither exercised nor observed, so its behaviour
drifts unnoticed until some future model or build routes around upstream and
puts it back in the path.

### 3. A real upstream defect, which gglib cannot reach and should not patch.

The regression arm reproduced a corruption 5/5: an argument value containing a
literal `</parameter>` absorbs the following parameter's markup, yielding
`path = "a</parameter>\n<parameter=mode>\ntext"` where the model demonstrably
intended `"a</parameter>b"` (its own `reasoning_content` says so).

Two things about it matter here:

- **It is not patchable as a boundary fix.** The generated grammar terminates
  values on a newline-anchored `\n</parameter>`; the model emits a bare
  `</parameter>`. The dialect has no escaping mechanism, so the byte sequence
  is genuinely ambiguous and every resolution is a heuristic. Written up for
  upstream as an issue with a reproducer rather than a patch.
- **Schema validation would not catch it.** The corrupted call satisfies every
  constraint: both required keys, correct types, enum respected, no extra
  properties. It is schema-valid and semantically wrong — the "form versus
  meaning" boundary, with a concrete instance on the far side.

An earlier version of the regression arm replayed the markup as assistant
*history* and reported that the server "accepted" it. That exercised nothing —
it proved only that llama-server carries arbitrary text in a prior message —
and it passed for the wrong reason. Recorded because a green result from a test
that cannot fail is worse than no test.

### 4. A second dialect family does not generalise — and shows where the gap is

The follow-up this ADR listed was run before anything was built on finding 1.
Same build, same harness, same schema; `Llama-3.2-3B-Instruct-UD-Q6_K_XL`:

```
  arm            calls   conformant     rate
  auto              30            4      13%
  required          30           30     100%
```

26 of 30 `auto` calls put `max_lines` as a **string** (`"42"`, `"null"`,
`"None"`) where the schema declares integer. The mechanism was visible in the
server log before the sweep ran:

| model | `tool_choice: auto` | `tool_choice: required` |
|---|---|---|
| Qwen3.5-4B | lazy grammar installed | grammar installed |
| **Llama 3.2 3B** | **no grammar** | grammar installed |

So the split is not model quality varying by prompt. It is the presence or
absence of enforcement. Where llama.cpp installs a grammar, arguments conform;
where it does not, a 3B model's own discipline is all there is.

**`auto` is the path every agentic client uses.** Cline, Roo Code, Zed and
Copilot BYOK all send it. On this model, gglib users receive type-wrong
arguments on the vast majority of tool calls with nothing in the stack
noticing.

The true rate is worse than the table states: the harness validates nested
*presence* but not nested *types*, so `options: {"follow_symlinks": "null"}`
went uncounted. Conformance is **≤ 13%**, not exactly 13%. A real validator
must recurse.

Finding 3's escaping gap also reproduced here in a harsher form. A value
containing `{` and `"` makes the peg-native parser fail and llama-server
returns **HTTP 500** — the whole request dies, not just the call. Silent
corruption on the XML dialect, a dead request on the JSON one, same root cause.

### 5. The repair mechanism is `tool_choice: "required"`

Finding 4 supplies its own remedy. Where `auto` is unconstrained and wrong,
`required` is constrained and right — on the same model, same build, same
schema, in the same run.

So gglib does not need to build a schema→GBNF converter to repair a bad call.
Upstream already has the enforcing machinery; it is simply not engaged on the
`auto` path. gglib's job is to engage it on the retry.

This is Tier B in its purest form: the policy (when to retry, and with what) is
gglib's, and the mechanism it drives is entirely upstream's.

## Decision

**1. The schema→GBNF constraint work is dropped, not deferred.** Upstream
enforces more than gglib planned to, on the path gglib actually runs. Building
it would duplicate working upstream machinery.

**2. `constrain.rs` has met its deletion criterion on the constraint
dimension** for this model and build. It is not deleted in this ADR: one model,
one build, one schema is not grounds for removing a stage that also serves
models and dialects not yet measured. Its module docs now record that the
criterion is met and what remains before removal.

**3. Feature #1 is re-scoped to its Tier B halves only** — proxy-side
validation and repair. Both are policy: llama-server has no view of what a
client does with a malformed call, so the decision to repair rather than
forward is gglib's regardless of how good upstream's grammar becomes.

**4. Validation is retained, and its value is not in doubt.**

*Superseded by the amendment.* The original text revised validation's expected
value **down**, on the reasoning that it was justified partly as insurance
against finding 3's class of defect and would not have caught it. That
judgement was drawn from a single model and did not survive the second: on
Llama 3.2, validation catches 26 of 30 `auto` calls. It is not insurance
against a hypothetical, it is detection of a live defect on the default path.

Recorded rather than quietly rewritten, because the mistake is the point —
a conclusion reached from one model read as a general one, which is the exact
failure ADR 0001 exists to prevent, committed inside the ADR enforcing it.

**5. Repair is `tool_choice: "required"` re-issue, not grammar origination.**
When validation fails on the `auto` path, gglib re-issues the turn with
`tool_choice: "required"`, which demonstrably installs upstream's grammar and
yields conformant arguments. Specified in
[Tool-call repair](../tool-call-repair.md).

## Consequences

**Good:**

- Work removed from the roadmap on evidence rather than intuition. The
  constraint half was the largest and riskiest third of Feature #1 —
  JSON Schema → GBNF is not a total function, and `$ref`, `anyOf` and
  recursion would each have needed a fallback path.
- ADR 0001's central rule paid for itself on first use: had gglib deferred on
  the capability flag alone it would have been right by luck, and had it built
  the constraint half without measuring it would have been wrong at
  considerable cost.
- The bypass finding gives Tier A a state nobody had named — *inert* — which
  the tier discipline now has to account for.

**Costs, accepted:**

- The result is scoped to one model, one build, one schema, 60 samples. A
  different dialect family, or a model whose template llama.cpp handles less
  well, could look nothing like this. Deferral decisions are per-capability and
  per-build, and this ADR licenses exactly one of them.
  *Confirmed by the amendment:* Llama 3.2 3B looks nothing like it. This caveat
  was written as a formality and turned out to be the load-bearing sentence in
  the document.
- gglib now has no defence against finding 3, because the corruption happens
  upstream of gglib and arrives pre-parsed. Contributing upstream is the only
  available lever, which is a slower loop than fixing it locally.
- A stage kept but known-redundant on the measured path is a maintenance
  liability until it is either deleted or re-validated on a second dialect.

## Follow-ups

- ~~Run the harness against a second dialect family before generalising
  finding 1.~~ Done — see findings 4 and 5. It did not generalise.
- Decide whether an inert Tier A module should be actively exercised (a
  synthetic markup path in tests) or allowed to go quiet — ADR 0001 has no
  answer for this.
- Submit the upstream issue (`upstream_issue_tool_dialect_escaping.md`); if a
  resolution lands, revisit findings 3 and 4.
- Record grammar-presence per model in `RuntimeCapabilities` once a third data
  point exists. Two models is a pattern, not a rule, and the harness reads it
  from a `--verbose` log rather than from anything gglib could query at
  runtime.
