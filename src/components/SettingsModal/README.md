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
| `InferenceProfileEditor.tsx` | Form for one profile. A blank parameter field is omitted from the payload rather than sent as `0`, so it falls through to the model's own default |

The advanced section is gated behind an `isAdvancedOpen` toggle to reduce visual complexity for new users.

Profiles are selected per request as `<model>:<profile>` and are global — one profile applies to every model. `profileNameError` mirrors the server's slug rule for immediate feedback only; the server validates independently and is the authority.

<!-- module-docs:end -->
