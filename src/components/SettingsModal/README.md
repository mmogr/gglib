# SettingsModal

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-SettingsModal-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-SettingsModal-complexity.json)

<!-- module-docs:start -->

Application settings modal: models directory path, base port configuration, per-request context size, default model selection, inference defaults, and advanced controls (tool iteration limit, title-generation prompt). Uses `InferenceParametersForm` for the defaults section.

## Key Files

| File | Role |
|------|------|
| `GeneralSettings.tsx` | Form body; directory, basic settings, default model, advanced section (collapsible), inference defaults |

The advanced section is gated behind an `isAdvancedOpen` toggle to reduce visual complexity for new users.

<!-- module-docs:end -->
