# constants

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-constants-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-constants-complexity.json)

<!-- module-docs:start -->

Application-wide shared constant values: fixed, non-environment-specific values used across UI components and the runtime layer.

## Key Files

| File | Role |
|------|------|
| `prompts.ts` | `DEFAULT_SYSTEM_PROMPT = 'You are a helpful assistant.'` — default chat system prompt; keeps runtime hooks free of UI-level defaults |
| `settingsDefaults.ts` | Default value and accepted range for each numeric settings-modal field, mirroring `Settings::with_defaults()` and `validate_settings()` in `crates/gglib-core/src/settings.rs` |
| `inferenceDefaults.ts` | Floor value and accepted range for each sampling parameter, mirroring `InferenceConfig::with_hardcoded_defaults()` and `validate_inference_config()` |

Adding constants here prevents duplication and ensures consistent defaults across tests, the backend API, and the UI.

<!-- module-docs:end -->
