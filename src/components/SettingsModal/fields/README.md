# fields

<!-- module-docs:start -->

Single-responsibility field groups for the General Settings form, plus the `SettingField` primitive they're built on.

`SettingField` renders one label / control / hint group and is where the placeholder-as-default problem is fixed once: settings inputs start empty and only backfill from the server when a value has been explicitly set, so an unset field previously showed nothing but its HTML placeholder — indistinguishable from a field the user hasn't typed in yet. `SettingField` accepts an explicit `defaultHint` and renders it as an always-visible "Default: 4096" line instead.

## Key Files

| File | Role |
|------|------|
| `SettingField.tsx` | Label + control + hint/default/action row |
| `PathSettings.tsx` | Models directory field and its exists/writable status pills |
| `ModelDefaults.tsx` | Default context size and default model selector |
| `PortSettings.tsx` | Proxy port, base server port, download queue size |
| `DisplaySettings.tsx` | Display-only toggles (currently: memory-fit indicators) |
| `AdvancedSettings.tsx` | Collapsible section: tool-iteration cap, title prompt, inference defaults |
| `SetupWizardRow.tsx` | Re-run the first-run setup wizard |

`GeneralSettings.tsx` (one level up) composes these in order; it holds no field-specific markup itself.

<!-- module-docs:end -->
