# Draft upstream issue: `/v1/chat/completions` unconditionally clobbers request-level `grammar_lazy` / `grammar_triggers` / `preserved_tokens`

**Status: draft, not yet filed.** Measured on build `7ba604f` (arm64 Metal),
Qwen3.5-4B Q8_0. Reproducer: `scripts/experiments/lazy_grammar_auto_probe.py`
(chat path) and its `/completion` positive control.

## Summary

A client of `/v1/chat/completions` can set `grammar` and have it honoured,
but cannot set `grammar_lazy`, `grammar_triggers`, or `preserved_tokens`:
the chat-template layer overwrites all three unconditionally with whatever
the template produced, even when the template produced no grammar at all.

`tools/server/server-common.cpp` (chat params → llama params):

```cpp
if (!chat_params.grammar.empty()) {          // <- guarded
    llama_params["grammar"]      = chat_params.grammar;
    llama_params["grammar_type"] = std::string("tool_calls");
}
llama_params["grammar_lazy"] = chat_params.grammar_lazy;   // <- unguarded
auto grammar_triggers = json::array();
for (const auto & trigger : chat_params.grammar_triggers) { ... }
llama_params["grammar_triggers"]  = grammar_triggers;      // <- unguarded
llama_params["preserved_tokens"]  = chat_params.preserved_tokens; // <- unguarded
```

With no `tools` in the request, `chat_params.grammar_lazy` is `false` and
the trigger list is empty, so a request carrying

```json
{
  "grammar": "root ::= \"<tool_call>\" ...",
  "grammar_lazy": true,
  "preserved_tokens": ["<tool_call>"],
  "grammar_triggers": [{"type": 1, "value": "<tool_call>"}]
}
```

runs its grammar **eagerly, from the first token**: a plain prose question is
force-fitted into the grammar's tool-call shape, and reasoning blocks are
suppressed entirely (the grammar cannot derive `<think>`).

## Expected

Request-level lazy-grammar fields honoured on the chat endpoint when the
template itself supplies no grammar — i.e. give the three companion fields
the same guard `grammar` already has, or merge request values when
`chat_params.grammar.empty()`.

## Measured

| arm | `/v1/chat/completions` | `/completion` (same fields) |
|---|---|---|
| demand a call | canary grammar engages (but eagerly) | model emits trigger, grammar engages **from the trigger** |
| plain prose | **hijacked into the grammar** | free prose, grammar never engages |

The `/completion` column shows the sampler-side mechanism working exactly as
documented; only the chat endpoint's parameter plumbing stands in the way.

## Why it matters

Proxies that add per-model, decode-time tool-call enforcement on top of
llama-server want exactly this: leave the model free until it *starts* a
call, then constrain — for model/template combinations where the template
emits no native grammar under `tool_choice: "auto"`. The pieces all exist
server-side; they are just unreachable through the OpenAI-compatible
endpoint.

## Two small adjacent findings

- A trigger object with a non-integer `type` (e.g. `{"type": "word"}`)
  fails `.get<int>()` inside the field handler; the request still returns
  200 with the grammar applied eagerly. A 400 naming the field would have
  saved a probe iteration.
- A single-special-token WORD trigger requires membership in
  `preserved_tokens`, which is reasonable but currently discoverable only by
  reading `server-schema.cpp`.
