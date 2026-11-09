# Status

<!-- module-docs:start -->

What a state is shown as. The words are written once, in
`gglib_core::domain::state_words`, printed by the CLI and recorded in
`contracts/vocabulary/words.json`; the maps here are held to that file by
`tests/ts/contracts/stateWords.test.ts`, and their key types are the
generated bindings, so a new state fails to compile until it has a word.
The shape beside a word is `StatusMark`: a state is always a shape plus a
word, never a colour alone.

| File | Purpose |
|------|---------|
| `words.ts` | `ENDPOINT_WORDS`, `RESIDENCY_WORDS`, `REACH_WORDS`, `KEY_WORDS`, `ENGINE_WORDS`, `TOOL_SERVER_WORDS`, `FAST_DOWNLOADS_WORDS`, `MODELS_DIR_WORDS`: the word for each `EndpointState`, `ResidencyState`, `ReachState`, `KeyState`, `EngineState`, `McpState`, `FastDownloadsState` and `ModelsDirState`; `CHOICE_WORDS`, the words for each `ChoiceSource` (whose choice a launch argument is, from `ChoiceSource::word`) |
| `StatusMark.tsx` | The shape for a state (`on` ● filled, `off` ○ hollow, `changing` ◌ an arc that turns, `away` ◌ amber, `attention` ▲, `failed` ✕) with the word beside it, or a label for readers when the shape stands alone; `blink`, the `on` dot's once-per-request blink; `REACH_MARK` and `RESIDENCY_MARK`, the shape for each `ReachState` and `ResidencyState` |
| `useRequestBlink.ts` | The endpoint dot's blink: on for 150 ms when the proxy's `total_requests` grows between two frames (`shouldBlink`), at most once a second, never for the first frame after the stream was idle, never under reduced motion; the sidebar's endpoint line and the tray's Endpoint block |

<!-- module-docs:end -->
