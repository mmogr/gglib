# contracts/upstream
What the pinned llama-server and sd-server answered, written by `make record-upstream` (`crates/gglib-runtime/src/engine_contract_tests.rs`).
Only `llama/approvals.json` is edited by hand; `make adr-readings` holds `llama/observed.json` to it.
`llama/bodies/` is copied in from a recording when a pin moves; its `apply-template.<template>.<thinking|no-thinking>.txt` are, verbatim, the prompts the pin renders for one fixed chat on each dateless template. `llama/tier3/` holds what Qwen3.5-0.8B did at the last llama.cpp bump, which no check reads. CONTRIBUTING's "The engine pins" says what a bump needs.
