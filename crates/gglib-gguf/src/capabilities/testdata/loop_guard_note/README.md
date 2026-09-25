# loop_guard_note

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-testdata-loop_guard_note-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/gglib-gguf-capabilities-testdata-loop_guard_note-complexity.json)

<!-- module-docs:start -->

Real chat templates, vendored **byte for byte** from llama.cpp `e5a8d439`
(`models/templates/`) — unlike the hand-reduced fixtures in the parent
directory, which model a family's tool-call markup rather than reproduce it.

The measurement that chose the delivery rendered 65 of that directory's 69
templates (4 do not compile in minijinja) against two conversation tails each.
Its harness was a throwaway and **the full table is not re-derivable from this
tree**; these five are the part of it that is, and they are the five that
decided the answer.

They are here because the loop guard's note (#1052) is delivered by appending
its text to the last message's content rather than as a trailing `system`
message, and these five are why. `loop_guard_note_templates_tests.rs` renders
each one through the same minijinja environment `template_probe` uses, so a
template update or a change to the delivery cannot quietly undo the finding.

- `Qwen3.5-4B.jinja` — raises `System message must be at the beginning.` on a
  trailing `system` message, which would be an HTTP 500 from llama-server
  where the guard used to return a clean 400. A model the owner runs.
- `mistralai-Mistral-Nemo-Instruct-2407.jinja` — raises on role alternation,
  the same failure for the Mistral family.
- `deepseek-ai-DeepSeek-V3.1.jinja` — concatenates every `system` message into
  a prompt prefix, so a note would land at token 0: read first rather than
  last, and breaking the cached prefix on every tripped turn.
- `openai-gpt-oss-120b.jinja` — drops a trailing `system` message silently.
- `microsoft-Phi-3.5-mini-instruct.jinja` — the pinned **known drop**: no
  branch for the `tool` role at all, so on an agentic tail it drops the last
  message whole and the in-content note with it. On a chat tail it renders the
  note in place.

<!-- module-docs:end -->
