# Status

<!-- module-docs:start -->

What a state is shown as. The words are written once, in
`gglib_core::domain::state_words`, printed by the CLI and recorded in
`contracts/vocabulary/words.json`; the maps here are held to that file by
`tests/ts/contracts/stateWords.test.ts`, and their key types are the
generated bindings, so a new state fails to compile until it has a word.

| File | Purpose |
|------|---------|
| `words.ts` | `ENDPOINT_WORDS`, `RESIDENCY_WORDS`, `REACH_WORDS`, `KEY_WORDS`: the word for each `EndpointState`, `ResidencyState`, `ReachState` and `KeyState` |

<!-- module-docs:end -->
