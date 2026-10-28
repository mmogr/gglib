# testdata

<!-- module-docs:start -->

The chat templates the engine recorder renders a fixed chat through,
byte for byte as llama.cpp ships them in `models/templates/` at the pinned
release (b11669), which is where `.github/actions/pinned-engines` fetches
them for each recording. `parity_tests.rs` renders each and compares the
result with what the pinned llama-server answered,
`contracts/upstream/llama/bodies/apply-template.<template>.<thinking|no-thinking>.txt`.

When the llama.cpp pin moves and those bodies are recorded again, these
move with them: fetch each from the new pin's `models/templates/`.

- `Qwen3.5-4B.jinja` - byte-equal to `capabilities/testdata/loop_guard_note/Qwen3.5-4B.jinja`
- `Qwen-Qwen3-0.6B.jinja`
- `ibm-granite-granite-4.0.jinja`

<!-- module-docs:end -->
