# contracts

Tests that pin the frontend to something outside it — a Rust signature, a wire shape, a value defined in the backend. Ordinary unit tests check that our code does what we meant; these check that what we meant still matches what the other side does.

They exist because this class of drift is silent. Nothing fails to compile when a Rust default changes, or when a command's payload gains a field. The GUI simply starts telling the user something that is no longer true.

| File | Pins |
|------|------|
| `startServerRequest.test.ts` | The flat `POST /api/servers/start` body against `StartServerRequest`'s field list and `StartServerBody`'s `alias`/`flatten` |
| `settingsBounds.test.ts` | `src/constants/settingsDefaults.ts` and `inferenceDefaults.ts` against `validate_settings`, `validate_inference_config`, `Settings::with_defaults` and `InferenceConfig::with_hardcoded_defaults` |
| `settingsParity.test.ts` | `STARTER_PROFILES` against `builtin_templates()`, and `MAX_STAGNATION_STEPS` against the agent default |

## Reading Rust from a TypeScript test

`settingsBounds.test.ts` parses the Rust source at run time. That is unusual enough to say why: the alternative is a comment asking the next person to keep two files in step, which is what was there before, and both values it guarded had already drifted — the GUI offered a Max Tokens default the backend deliberately does not have, and a Repeat Penalty of 0 that validation rejects.

Two rules keep the parsing honest:

- **Anchor on the guard, not the name.** Scanning forward from a bare field name reads whatever comes next, including the following parameter's bounds. An early draft passed the Repeat Penalty bug for exactly this reason.
- **Throw, never default.** Every extractor names the symbol it could not find. Restructuring the Rust turns the test red rather than quietly retiring the guarantee it was providing.

The check is a subset relation, not equality: the GUI may be stricter than the backend (several caps are deliberate UI guard rails over a Rust bound that does not exist), but never looser.
