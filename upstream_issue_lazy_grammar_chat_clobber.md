# server: `/v1/chat/completions` ignores request-level `grammar_lazy` / `grammar_triggers` / `preserved_tokens` (template output overwrites them unconditionally)

**Version:** build `7ba604f`, macOS arm64 (Metal). Model used to reproduce:
Qwen3.5-4B (Q8_0), launched `--jinja -c 8192 -ngl 99 --reasoning-format deepseek`.

## Summary

On `/v1/chat/completions`, a client can set `grammar` and have it honoured,
but cannot set `grammar_lazy`, `grammar_triggers`, or `preserved_tokens`:
the chat-template layer overwrites all three with the template's own values
even when the template produced **no grammar at all**. The result is that a
request-level grammar always runs **eagerly** on the chat endpoint — there
is no way to request lazy engagement through the OpenAI-compatible API.

In `tools/server/server-common.cpp` (chat params → llama params):

```cpp
if (!chat_params.grammar.empty()) {          // <- guarded
    llama_params["grammar"]      = chat_params.grammar;
    llama_params["grammar_type"] = std::string("tool_calls");
}
llama_params["grammar_lazy"] = chat_params.grammar_lazy;          // <- unguarded
auto grammar_triggers = json::array();
for (const auto & trigger : chat_params.grammar_triggers) { ... }
llama_params["grammar_triggers"]  = grammar_triggers;             // <- unguarded
llama_params["preserved_tokens"]  = chat_params.preserved_tokens; // <- unguarded
```

With no `tools` in the request, `chat_params.grammar_lazy` is `false` and
`chat_params.grammar_triggers` is empty, so the request's own values are
clobbered after being parsed.

## Reproduction

Grammar used in both cases (forces a canary call so engagement is visible
in the output):

```
root ::= "<tool_call>" nl "{\"name\": \"canary_fn\", \"arguments\": {\"x\": " int "}}" nl "</tool_call>"
int ::= [0-9]+
nl ::= "\n"
```

**Case 1 — `/v1/chat/completions`, plain prose question:**

```bash
curl -s http://127.0.0.1:15600/v1/chat/completions -H 'Content-Type: application/json' -d '{
  "messages": [{"role": "user", "content": "In one short sentence, what is a lighthouse for?"}],
  "max_tokens": 200,
  "grammar": "root ::= \"<tool_call>\" nl \"{\\\"name\\\": \\\"canary_fn\\\", \\\"arguments\\\": {\\\"x\\\": \" int \"}}\" nl \"</tool_call>\"\nint ::= [0-9]+\nnl ::= \"\\n\"",
  "grammar_lazy": true,
  "preserved_tokens": ["<tool_call>"],
  "grammar_triggers": [{"type": 1, "value": "<tool_call>"}]
}'
```

Actual: the prose question is force-fitted into the grammar from the first
token —

```
<tool_call>
{"name": "canary_fn", "arguments": {"x": 100}}
</tool_call>
```

— and on a thinking model the reasoning block is suppressed entirely (the
grammar cannot derive `<think>`). Expected: free generation until the model
itself emits `<tool_call>`, per the `grammar_lazy` field's description.

**Case 2 — `/completion`, identical fields (control):** the same body shape
against `/completion` behaves exactly as documented — a prose prompt
generates freely and never engages the grammar; a prompt that leads the
model to emit `<tool_call>` engages it from the trigger onward and produces
a conformant canary call. So the sampler-side mechanism is fine; only the
chat endpoint's parameter plumbing is in the way.

## Expected behaviour / suggested fix

Give the three companion fields the same guard `grammar` already has — only
apply the template's `grammar_lazy` / `grammar_triggers` /
`preserved_tokens` when the template actually produced a grammar (or merge
request-supplied values when `chat_params.grammar.empty()`). That keeps
template-driven tool-call enforcement exactly as it is today while letting
API clients use lazy grammars on chat for model/template combinations where
the template emits no native grammar under `tool_choice: "auto"`.

## Why this matters

Middleware adding per-model, decode-time tool-call enforcement on top of
llama-server wants precisely this shape: leave the model free until it
*starts* a call, then constrain — for templates that install nothing under
`auto`. Every server-side piece already exists; it is only unreachable
through the OpenAI-compatible endpoint.

## Two small adjacent findings

- A trigger object with a non-integer `type` (e.g. `{"type": "word"}` — a
  plausible spelling) fails `.get<int>()` inside the field handler, and the
  request still returns 200 with the grammar applied eagerly. A 400 naming
  the field would make the contract discoverable.
- A WORD trigger that tokenizes to a single special token requires
  membership in `preserved_tokens` (reasonable), but this is currently
  discoverable only by reading `server-schema.cpp`.
