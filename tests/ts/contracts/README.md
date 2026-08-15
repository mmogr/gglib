# contracts

Tests that pin the frontend to something outside it — a Rust signature, a wire shape, a value defined in the backend. Ordinary unit tests check that our code does what we meant; these check that what we meant still matches what the other side does.

They exist because this class of drift is silent. Nothing fails to compile when a Rust default changes, or when a command's payload gains a field. The GUI simply starts telling the user something that is no longer true.

| File | Pins |
|------|------|
| `startServerRequest.test.ts` | The flat `POST /api/servers/start` body against `StartServerRequest`'s field list, `StartServerBody`'s `alias`/`flatten`, and `InferenceConfig`'s accepted keys |
| `settingsBounds.test.ts` | `src/constants/settingsDefaults.ts` and `inferenceDefaults.ts` against `validate_settings`, `validate_inference_config`, `Settings::with_defaults` and `InferenceConfig::with_hardcoded_defaults` |
| `settingsParity.test.ts` | `STARTER_PROFILES` against `builtin_templates()`, and `MAX_STAGNATION_STEPS` against the agent default |

`rustSource.ts` is not a test. It holds the extractors all three share — `rust`, `declaration`, `structBody`, `fnSource`, `scannable`, `withoutComments` — and with them the half of the first rule that can be shared: locating a declaration by an anchored, uniqueness-checked match rather than a first-match `indexOf`. Three rounds of review found the same decoy defeating whichever extractor had not been fixed yet, which is why it is one module and not three copies. Anchoring on a *particular* guard, and throwing rather than defaulting, still belong to each test.

## Reading Rust from a TypeScript test

Every test here parses the Rust source at run time. That is unusual enough to say why: the alternative is a comment asking the next person to keep two files in step, which is what was there before, and in each case the thing it guarded had already drifted — `settingsBounds.test.ts` found a Max Tokens default the backend deliberately does not have and a Repeat Penalty of 0 that validation rejects, and `startServerRequest.test.ts` replaced a test that pinned an IPC command which had never existed.

Two rules keep the parsing honest:

- **Anchor on the guard, not the name.** Scanning forward from a bare field name reads whatever comes next, including the following parameter's bounds. An early draft passed the Repeat Penalty bug for exactly this reason.
- **Throw, never default.** Every extractor names the symbol it could not find. Restructuring the Rust turns the test red rather than quietly retiring the guarantee it was providing.

The check is a subset relation, not equality: the GUI may be stricter than the backend (several caps are deliberate UI guard rails over a Rust bound that does not exist), but never looser.
