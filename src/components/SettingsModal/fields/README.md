# fields

<!-- module-docs:start -->

Single-responsibility field groups for the General Settings form, plus the `SettingField` primitive they're built on.

`SettingField` renders one label / control / hint group. Settings inputs start empty and only backfill from the server when a value has been explicitly set, so what leaving a field empty does has to be stated rather than inferred, and it is stated twice on purpose: `SettingField` renders an always-visible line below the control, and the control carries a matching placeholder. The hint survives focus and survives a value being entered; the placeholder sits in the box where it can be typed over.

Numeric fields get both from one `NumericSettingSpec` through `NumberSettingField`. That spec has two shapes, and the type enforces the choice. A field with a fixed backend default (`{default: '8080', min, max}`) is captioned `Default: 8080`. A field with none (`{default: null, unset: {placeholder, hint}, min, max}`) renders its `hint` as a plain sentence and its shorter `placeholder` in the box — never behind a "Default:" label, because captioning a field with a default it does not have is the contradiction the shape exists to prevent. The context window is the only field of the second kind: it is resolved per launch from the model and the machine, or from the built-in floor on a host whose device gglib cannot read.

A field that supplies its own control (the models directory, the title prompt) owns its placeholder itself.

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
| `SecuritySettings.tsx` | API-key field for the proxy, over the shared `SettingField` |

`GeneralSettings.tsx` (one level up) composes these in order; it holds no field-specific markup itself.

<!-- module-docs:end -->
