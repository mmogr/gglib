# fields

<!-- module-docs:start -->

Single-responsibility field groups for the General Settings form, plus the `SettingField` primitive they're built on.

`SettingField` renders one label / control / hint group. Settings inputs start empty and only backfill from the server when a value has been explicitly set, so a field's default has to be stated rather than inferred, and it is stated twice on purpose: `SettingField`'s `defaultHint` renders an always-visible "Default: 4096" line, and the control carries the same value as its placeholder. The hint survives focus and survives a value being entered; the placeholder sits in the box where it can be typed over. Numeric fields get both from a single `{default, min, max}` spec through `NumberSettingField` — a field that supplies its own control (the models directory, the title prompt) owns its placeholder itself.

## Key Files

| File | Role |
|------|------|
| `SettingField.tsx` | Label + control + hint/default/action row |
| `NumberSettingField.tsx` | `SettingField` plus the number input itself, wired to a `{default, min, max}` spec from `src/constants/settingsDefaults.ts`; every numeric setting renders through it |
| `ToggleField.tsx` | Checkbox + bold label + explanatory paragraph; every boolean setting renders through it |
| `PathSettings.tsx` | Models directory field and its exists/writable status pills |
| `ModelDefaults.tsx` | Default context size and default model selector |
| `PortSettings.tsx` | Proxy port, base server port, download queue size |
| `DisplaySettings.tsx` | Display-only toggles (currently: memory-fit indicators) |
| `DesktopSettings.tsx` | Always-on proxy group: autostart, close-to-tray, start-at-login |
| `AdvancedSettings.tsx` | Collapsible section: tool-iteration cap, title prompt, inference defaults |
| `SetupWizardRow.tsx` | Re-run the first-run setup wizard |

`GeneralSettings.tsx` (one level up) composes these in order; it holds no field-specific markup itself.

<!-- module-docs:end -->
