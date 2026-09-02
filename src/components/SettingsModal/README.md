# SettingsModal

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-SettingsModal-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-SettingsModal-complexity.json)

<!-- module-docs:start -->

Application settings modal: models directory path, base port configuration, per-request context size, default model selection, inference defaults, named inference profiles, and advanced controls (tool iteration limit, title-generation prompt). Uses `InferenceParametersForm` for the defaults section.

## Key Files

| File | Role |
|------|------|
| `GeneralSettings.tsx` | Form body; directory, basic settings, default model, advanced section (collapsible), inference defaults |
| `InferenceProfiles.tsx` | Profiles tab: lists named sampling profiles with add/edit/delete. Self-contained — loads and saves settings itself rather than threading state through `SettingsModal`, matching `McpServersPanel` |
| `useDesktopSettings.ts` | State for the three always-on proxy toggles, kept out of `SettingsModal` so the group owns its own state and update payload |
| `InferenceProfileEditor.tsx` | Form for one profile. A blank parameter field is omitted from the payload rather than sent as `0`, so it falls through to the model's own default |
| `SystemSettings.tsx` | System tab — the GUI face of `gglib config llama` |
| `DiagnosticsPanel.tsx` | `gglib config check-deps`, `paths` and `fast-downloads status` as one panel in the System tab |
| `LabelledValue.tsx` | A label and its value, for the read-only rows those two panels are mostly made of |
| `BuildInfo.tsx` | The daemon's own build — version and commit — read from `/api/version` |
| `useSystemSettings.ts` | State for the System tab: what llama.cpp is installed, whether upstream has moved, and the update/uninstall actions |
| `useDiagnostics.ts` | State for the diagnostics half of the System tab |
| `useNetworkSettings.ts` | State for the network-binding settings (bind host, LAN sharing) |
| `useAgentGuardSettings.ts` | State for the agent-guard settings (agentic sampling cap, stagnation limit) |
| `fields/` | The reusable field primitives these panels are built from |

The advanced section is gated behind an `isAdvancedOpen` toggle to reduce visual complexity for new users.

Profiles are selected per request as `<model>:<profile>` and are global — one profile applies to every model. `profileNameError` mirrors the server's slug rule for immediate feedback only; the server validates independently and is the authority.

<!-- module-docs:end -->
