# render

<!-- module-docs:start -->

A chat rendered through its template the way the pinned llama-server
renders it, so gglib's in-process engine builds the prompt llama-server
would.

[`render_chat`] gives a template exactly what llama-server's Jinja path
gives it at the pin (`common_chat_template_direct_apply_impl`,
`common/chat.cpp:877-951` at llama.cpp b11669): the messages shaped as it
shapes them, the vocabulary's `bos_token` and `eos_token`, `enable_thinking`,
`datetime` and `date_string`, every template kwarg (with llama-server's
launch default `preserve_reasoning: true` and the variables it sets), and
`add_generation_prompt` only when true. No `tools` are given. The
environment behaves as llama's own engine where a chat template can tell:
trimmed and left-stripped blocks, chainable undefined values, a
`strftime_now` on the local clock, `raise_exception`, booleans printed
`True` and `False`, and `tojson` writing what Python's `json.dumps` writes.

`parity_tests.rs` holds every render to what the pinned llama-server's
`POST /apply-template` answered for the same chat, recorded under
`contracts/upstream/llama/bodies/`.

A request's `reasoning_budget_tokens` of 0 renders with thinking off. That
rule is gglib's, not llama-server's (which ends thinking at its sampler),
and it lives in [`ChatKwargs::from_request`].

Deliberately left out, because each rests on llama's own analysis of which
inputs a template reads: a system message merged into the next turn for a
template with no system role (`common/chat.cpp:1339-1341`), text turned
into typed content parts for a template that reads only those
(`common/chat.cpp:507-518`), and the default `enable_thinking` for a
template whose flag changes something other than a reasoning block (see
[`ChatKwargs`]). Messages are text. A mapping built in a template keeps its
order, but one inside a kwarg reaches the template with its keys sorted,
where llama-server keeps the request's order.

Also left out: the GGUF's `tokenizer.chat_template.tool_use`, which
llama-server renders in place of `ChatML` when the default template is
empty or `chatml` (`common/chat.cpp:744-758`), and the marking of a
message's text as user input, by which llama-server leaves a leading BOS
or trailing EOS in place when a message's own text supplies it
(`common/chat.cpp:938-949`). Here the tokenizer's BOS or EOS is dropped
wherever it comes from.

# Structure

- `mod.rs` - `render_chat`, `ChatTemplate` (the source llama-server serves,
  with its `ChatML` fallback and two upstream patches), `VocabTokens`,
  `ChatMessage`, `RenderError`, and the variables in llama-server's order
- `kwargs.rs` - `ChatKwargs`: a request's `chat_template_kwargs`,
  `reasoning_effort`, and the zero-budget rule
- `messages.rs` - each message as llama-server shapes it for a template
- `env.rs` - the minijinja environment and `strftime_now`
- `tojson.rs` - `tojson` as Python's `json.dumps`
- `tests.rs` - the inputs, one at a time
- `parity_tests.rs` - the recorded renders, byte for byte
- `testdata/` - the three pinned templates, at the pin

<!-- module-docs:end -->
