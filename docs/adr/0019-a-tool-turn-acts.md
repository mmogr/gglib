# ADR 0019 — A tool turn acts: the proxy gives a request with tools whose last message is not the user's a reasoning budget of zero, where nobody chose how much it thinks

- **Status:** Accepted
- **Date:** 2026-10-13
- **Depends on:** [ADR 0004](0004-observe-the-sampling-boundary.md),
  [ADR 0006](0006-recover-dont-predict.md),
  [ADR 0007](0007-ask-the-server-for-template-capabilities.md)
- **Supersedes:** nothing
- **Superseded by:** nothing
- **Log:** none

## Context

An agentic client sends its `tools` on every request and replays the whole
conversation each turn. Most of those requests are not a person asking
something: they are the client handing back a tool result so the model can
take its next step. A model that thinks spends a reasoning block on each one.

### Two runs of one task

Taken 2026-10-13. One task (a read-only "plan the fix" brief for the bug
[#1298](https://github.com/mmogr/gglib/pull/1298) fixed, against the commit
before that fix), one model (Qwen3.8-27B), one quantisation (Q8_0), one client
(Copilot CLI in plan mode, with three read-only tools), through gglib's proxy,
on one machine, **one run of each arm**. The plans were graded by hand against
the real fix.

| | Run A, thinking on | Run B, thinking off |
|---|---|---|
| Wall time | 43 min 25 s | 14 min 22 s |
| Turns | 14 | 50 |
| Completion tokens | not recorded | 6,389 |
| Tool calls (turns with more than one) | 14 (0) | 59 (10) |
| Plan, graded against the fix | right function and both callers; one wrong design call | right function and both callers; that design call right; one site run A found, missed |

In run B a shim in front of the proxy added
`chat_template_kwargs.enable_thinking=false` to **every** request. So run B is
thinking off on every turn, by the template switch. It is not a run of the
policy this ADR decides, and no such run has been taken.

Source: `runs/1298/GRADE.md` and `runs/1298/think-off/requests.jsonl` in the
session's evaluation folder (`~/.local/src/gglib-eval` on the machine that took
them). **The raw runs are not in this repository**, so the table above is the
record and it cannot be re-derived from the repository alone.

### Which turns were tool turns

Of run B's 50 requests, the first carried two messages (the system prompt and
the person's task). Each of the other 49 followed a turn that finished with
`tool_calls` and had grown by that turn's assistant message and its tool
results. The shim logged message counts and finish reasons, not roles, so "49
of 50 ended in a tool result" is read from those two columns, not from the
role of the last message.

Run A's prompt cache held (about 42k of 45k prompt tokens served from cache on
a sampled turn), so the time was generation: reasoning before each of 14 tool
calls.

### The mechanism

llama-server takes a per-request `reasoning_budget_tokens`. Read from
llama.cpp at commit `972b5fc`:

- `tools/server/server-common.cpp:1439-1450` passes the request's budget (and
  `reasoning_budget_message`) to sampling only when the rendered template has
  thinking end tags.
- `common/sampling.cpp:315` builds the budget sampler only when a start and an
  end tag both exist.
- `common/reasoning-budget.cpp:74-88`: the sampler is idle until it sees the
  start tag, and with a budget of `0` it forces the end tag at once.

So a budget sent to a model whose template does not think is read and does
nothing, and on one that thinks it is enforced by the sampler. A template
variable such as `enable_thinking` is read by some model families and ignored
by others, in silence, as [ADR 0007](0007-ask-the-server-for-template-capabilities.md)
finding 7c records for `reasoning_effort`.

Measured the same day, one tool-carrying request (two tools, a system and a
user message) sent straight to llama-server at that commit, Qwen3.8-27B Q8_0,
six ways, **one run per row** except budget `0`, which has two:

| Variant | Reasoning chars | Tool calls | Completion tokens |
|---|---|---|---|
| default | 163 | 2 | 98 |
| `chat_template_kwargs.enable_thinking=false` | 0 | 2 | 68 |
| `reasoning_budget_tokens: 0` | 0 | 2 | 65 |
| `reasoning_budget_tokens: 0` (repeat) | 0 | 2 | 75 |
| budget 512 with a budget message | 134 | 2 | 109 |
| budget 64 with a budget message | 189 | 2 | 110 |
| `reasoning_effort: "none"` | 0 | 2 | 71 |

Budget `0` produced no reasoning in 2 of 2 requests, as the template switch
did in 1 of 1. The source records the budget-64 row's 189 characters as under
64 tokens, and the budget-512 row thought for less, so no row exercised the
cut-off or the message.

`reasoning_effort: "none"` also produced no reasoning here, in 1 of 1. The
budget was still chosen:
[ADR 0007](0007-ask-the-server-for-template-capabilities.md) decision 4 does
not offer `"none"` as a level, because its finding 4 shows it giving medium
thinking on gpt-oss and doing nothing on a template that does not read
`enable_thinking`.

The script that sent these (`budget-test.sh`) was in that session's scratch
folder. This ADR does not know whether it still exists; the source says the
table's first column reproduces the request bodies. Source:
`research/budget-mechanism.md` in the same evaluation folder.

### What was already here

- `request_pipeline::sampling::resolve_sampling` already knows a turn that
  carries tools, and caps its temperature after the ladder is folded, only
  over a value nobody chose ([ADR 0004](0004-observe-the-sampling-boundary.md),
  its addendum on task overlays and the postscript under it).
- Every gglib surface already means "thinking off" by
  `reasoning_budget_tokens: 0` ([Thinking](../clients.md#thinking)), and a
  client's own budget is honoured whatever `trust_client_sampling` says.

## Decision

1. **The proxy has a thinking policy for a request that carries tools**, one
   setting, `agentic_thinking`, with four values:

   | Value | What a request with tools is sent |
   |---|---|
   | `default` | nothing: gglib sets no budget |
   | `off` | `reasoning_budget_tokens: 0` |
   | `brief` | `reasoning_budget_tokens: 512` and `reasoning_budget_message: "Enough thinking. Act now."` |
   | `when-asked` | `reasoning_budget_tokens: 0` unless the last message has the role `user`; nothing when it has |

2. **With nothing stored, the policy is `when-asked`.** It differs from run B
   on the turn where a person has just spoken, which is left to think. In run
   B that was 1 turn of 50, by the reading in Context. The reason for keeping it is not measured: it is
   that a person's message is where the task is set, and the 49 turns after it
   are where the time went.

3. **The control is the budget, not a template variable.** gglib is used with
   any model, and the budget is the one switch llama-server enforces itself
   and ignores where there is nothing to switch (Context, The mechanism). It
   is also the key gglib already means thinking-off by. gglib sends no
   `chat_template_kwargs`.

   The policy does not ask whether the model is tagged `reasoning`. The tag is
   detected at import and can be absent on a model that thinks, and the budget
   is inert on one that does not.

4. **It acts only where nobody chose how much the turn thinks**, by the rule
   the temperature ceiling uses (`ParamSource::is_deliberate_choice`). Three
   things are that choice, and the policy sets no budget on a request with any
   of them:

   - *A resolved budget from a rung a person set*: the client's request, a
     `gglib proxy --reasoning-budget-tokens` flag, a profile, a model's own
     user-set defaults or the global defaults. So is one a tune sweep measured
     for the model. A budget written at import, or published by the model's
     author, is no choice and is replaced.
   - *A budget in the client's request that gglib could not read*, for
     example a number below `-1` or a string, under
     `reasoning_budget_tokens` or its alias `thinking_budget_tokens`. The
     request then goes on exactly as it does under `default`: the canonical
     key is forwarded as the client wrote it, which llama-server answers with
     a 400 for a number below `-1`, unless a stored budget is written over
     it; the alias is removed, as it is from every request.
   - *A resolved `reasoning_effort` from a rung a person set*, with no budget
     beside it: the same rungs, the client's among them only while
     `trust_client_sampling` is on. With it off the ladder discards a client's
     effort, as it always has, and the policy still acts. The effort is
     judged by the rung the fold named, in stage 4. The template gate of
     [ADR 0007](0007-ask-the-server-for-template-capabilities.md) runs after
     it, in stage 5b, and may still delete an effort the model's template does
     not read; the policy has stood down by then, so that turn is sent neither
     the effort nor a budget.

   **A client opts out by sending its own `reasoning_budget_tokens`**: `-1`
   for the model's launch default, or any whole number above it. With
   `trust_client_sampling` on, its `reasoning_effort` does the same when it is
   a level gglib reads (`minimal` to `max`). `"none"` is refused by the reader
   and is no choice.

5. **The provenance says so.** A budget the policy set is recorded as
   `ParamSource::AgenticTurn`, never as the rung or the floor the fold named,
   and `SamplingDecision::agentic_budget_applied` carries the number.

6. **It is the proxy's only.** The built-in agent (the chat page, `gglib
   chat`, `gglib q`) and a paired device's turn pass the policy that changes
   nothing, and keep the per-chat Thinking switch they already have. This is a
   choice, not an oversight: those chats have a person who can press the
   switch. Agentic work started from a phone is where the policy may be wanted
   next.

   When the built-in agent's model is on a paired machine, its requests go
   to that machine's proxy, which has this policy. There the agent opts out
   as any client does. A request with tools
   that names no budget of its own is sent `reasoning_budget_tokens: -1`;
   Thinking off is sent as the `0` it always was. The cost: a client's budget
   outranks that machine's stored ones, so a budget its profile, model or
   global defaults name does not reach such a turn. The far machine's
   `--reasoning-budget-tokens` flag still does, since the flag outranks a
   client.

   `gglib chat --port` and `gglib q --port` take the port of a llama-server.
   Given a proxy's port instead, the agent is a client of that proxy like any
   other and sends no opt-out, so the policy applies to its turns.

7. **It shares the agentic switch.** `agentic_sampling` stored off, or
   `GGLIB_DISABLE_AGENTIC_SAMPLING` set in the process that shapes the
   request, turns the policy off with the temperature ceiling.

### Why this is not the overlay ADR 0004 refused

[ADR 0004](0004-observe-the-sampling-boundary.md)'s addendum refuses
request-level task overlays on three arguments. None is met here, and the
difference is recorded so that this ADR is not read as licence for one.

- *The single-completion dilemma*: a temperature chosen for the tool call
  lands on the reasoning before it, since both are one completion under one
  sampler configuration. This policy sets no sampler value. The budget acts
  on the reasoning block and on nothing after it.
- *The classifier cannot know what it claims to know*: `carries_tools` says
  whether a turn could emit a call, never whether it will. The policy does
  not claim it will. It reads who wrote the last message, by role, and
  nothing about the content.
- *A ladder rung would have taken the coupled trio with it*: a rung that
  names a `temperature` claims the coupled trio, so `presence_penalty`,
  `repeat_penalty` and `min_p` then come only from that rung. The policy is
  not a rung and names no temperature. It writes one value after the fold, as the ceiling does, and
  the reasoning budget is coupled to nothing.

[ADR 0006](0006-recover-dont-predict.md) removed a scheduler that chose
sampling from past traffic. This reads only the request in hand and learns
nothing. It is still a choice made before the output exists, which is why it
has a switch, an opt-out and the criteria below.

### Where it lives

- [`settings_agentic_thinking.rs`](../../crates/gglib-core/src/settings_agentic_thinking.rs):
  the four values, the 512 and the message, and the rule that maps a policy
  and a last speaker to a budget.
- [`request_pipeline/request_shape.rs`](../../crates/gglib-core/src/request_pipeline/request_shape.rs):
  `carries_tools` and `LastSpeaker`, the two readings, and `TurnShape`, which
  holds them together.
- [`request_pipeline/apply.rs`](../../crates/gglib-core/src/request_pipeline/apply.rs):
  reads who spoke last from the request as the client sent it, before any
  stage changes a message.
- [`request_pipeline/sampling.rs`](../../crates/gglib-core/src/request_pipeline/sampling.rs):
  `resolve_sampling` reads whether the request still carries tools and
  applies the policy, after the temperature ceiling.
- [`domain/sampling_provenance.rs`](../../crates/gglib-core/src/domain/sampling_provenance.rs):
  `ParamSource::AgenticTurn`.
- [`gglib-proxy/src/server.rs`](../../crates/gglib-proxy/src/server.rs): the
  one caller that passes the stored policy. It also reads who spoke last
  before the loop guard's note goes on, since the note can be a new `user`
  message.
- [`llm_completion/mod.rs`](../../crates/gglib-runtime/src/ports_impl/llm_completion/mod.rs):
  the built-in agent's opt-out on a paired machine's proxy (decision 6).

## Consequences

- **A person using an agentic client through the proxy is expected to see
  turns after a tool result start acting sooner**, on a model that thinks,
  with no change to the client. No run of `when-asked` exists. On the one
  task measured, with thinking off on every turn, the run was three times
  faster for a plan graded equal or slightly better.
- **The evidence is one task, one model, one quantisation, one client, one run
  of each arm, and it is for a different arm than the default.** Nothing here
  shows the result holds for a task that needs deeper reasoning between tool
  calls, or for another model.
- **Published evidence found points the other way for another model.** The
  survey taken with the runs (`research/web-survey.md`, not in this
  repository) records two things this ADR did not re-read at their sources:
  the Qwen3.8-27B model card's warning that in multi-turn agentic tasks lower
  reasoning effort "does not always reduce overall task completion time" and
  can lead to "more failures, and repeated retries"; and three published
  measurements of an agentic task at more and less thinking. gpt-oss-120b on
  SWE-bench Verified scored 47.9, 52.6 and 62.4 at low, medium and high
  effort, which the survey calls the cleanest public measurement. A study by
  one author on one task found first-try perfect runs rising from 28% to 89%
  between high and xhigh effort. A sweep of Claude's thinking budget on
  SWE-bench Verified was flat above 8k: 67.3, 68.4 and 68.7 at 8k, 16k and
  32k. The first two point against less thinking; the third found no gain
  from more of it above 8k.
- **"Who spoke last" is a role, not a fact about a person.** A client that
  appends a `user`-role note after its tool results makes every such turn read
  as the person's, and `when-asked` then leaves them all alone. A client that
  sends tool results as `user` messages does the same. `off` does not depend
  on the role. The role is read from the request as the client sent it: a
  `system` message gglib rewrites as a `user` one for a model with no system
  role, and the loop guard's own note, are not the person speaking.
- **Nothing on the wire confirms the budget arrived.** `/slots` and `/props`
  echo neither reasoning control
  ([ADR 0007](0007-ask-the-server-for-template-capabilities.md) finding 7a).
  The record is gglib's own: `sampling_audit.reasoning.latest.budget` in
  `GET /v1/proxy/status` (`source: "agentic turn"`), which holds the newest
  request and no other; the `sampling resolved` debug line
  (`agentic_budget`); and what each turn then generated, in
  `recent_requests[]`.
- **`brief`'s number is a choice.** No run reached a 512-token cut-off, so
  neither the number nor the message has been seen to do anything.
- **`gglib model explain` and the GUI's sampling panel cannot show it.** They
  explain stored configuration with no request in hand.
- **`gglib benchmark agentic`'s proxy arm runs under it.** The arm's fixed
  settings name `when-asked`, so a later change of the unset default does not
  move a stored comparison, and the report records the policy beside the
  loop-guard mode. A report stored before this has no such field. The arm's
  difference from raw-auto is now repair, the loop guard and this policy
  together.
- **More turns.** Run B took 50 turns to run A's 14. A client that bills or
  limits by request count sees that.

## Kill criteria

1. **The budget does not stop the thinking on some model.** Reading: in
   `GET /v1/proxy/status`, taken straight after one request and before the
   next, `sampling_audit.reasoning.latest.budget` reads `tokens: 0`,
   `source: "agentic turn"`, and the newest row of `recent_requests[]`, which
   is that same request, has `reasoning_chars` above `0`. `latest` is one
   record, the newest request's, so it says nothing about an older row. One
   such turn, on a named model and llama.cpp build, means decision 3's claim
   does not hold for that model, and that model needs another control or an
   exemption. Reading
   so far: `0` reasoning characters in 2 of 2 budget-`0` requests, direct to
   llama-server, one model (Context). Through the proxy: no reading taken.

2. **The default is not faster.** Reading: a graded rerun of the task in
   Context under `when-asked`, same model, client and brief, timed by wall
   clock, with each turn's `generation_ms` and `reasoning_chars` read from
   `recent_requests[]` as the run goes (the status shows the newest 20 rows).
   A wall time above 21 min 42 s (half of run A's) reopens the default. The
   half is a threshold this ADR chose; nothing measured it.
   Reading so far: none; the run has not been taken.

3. **The default costs quality.** Reading: the same graded rerun, or a graded
   pair (thinking on against `when-asked`) on any second task or model. A plan
   or patch graded worse than its thinking-on pair on a criterion that pair
   met reopens the default for the model it was measured on. Two such pairs on
   different models return the unset default to `default`. One pair and two
   pairs are thresholds this ADR chose; nothing measured them. Reading so far: 0
   pairs taken under `when-asked`; 1 pair under thinking off on every turn,
   graded equal or slightly better (Context).

4. **A client defeats the last-speaker reading.** Reading: with one agentic
   session on the proxy, the policy `when-asked` and no budget from the
   client, the `recent_requests[]` rows that come straight after a row with
   `finish_reason: "tool_calls"` have `reasoning_chars` above `0`. Such a row
   is the client handing back tool results, so thinking there means the
   policy set no budget on it, or set one that did not hold, which is
   criterion 1. Of the reasons it set none, one is this criterion's: the request's
   last message read as the user's, because the client appends `user`
   messages after its tool results, and `when-asked` does nothing for it. The
   others are decision 4's and have to be ruled out first: a budget stored on
   a profile, the model or the global defaults, or given by the proxy's flag;
   an effort level a person set on any of those; and, with
   `trust_client_sampling` on, an effort the client sent. The `sampling
   resolved` debug line names the rung of each. Reading so far: none.

## Out of scope

- The built-in agent and paired devices (decision 6).
- Launch flags (`--reasoning-budget`), which would reach a client that
  bypasses the proxy.
- `reasoning_effort`, which stays a template control with its own gate
  ([ADR 0007](0007-ask-the-server-for-template-capabilities.md)). The policy
  sets none; it reads only whether somebody chose one (decision 4).
- Ending a reasoning block that is already running.
- Any text added to the prompt.
