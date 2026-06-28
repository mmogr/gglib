# InferenceParametersForm

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-InferenceParametersForm-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-InferenceParametersForm-complexity.json)

<!-- module-docs:start -->

Tristate inference parameter form where each field can be `undefined` (inherit parent default), `null` (explicitly unset), or a concrete number (override). Shows reset buttons when a field is explicitly set and displays the effective inherited value as a placeholder hint.

## Key Files

| File | Role |
|------|------|
| `InferenceParametersForm.tsx` | Number inputs for temperature, top-p, max-tokens, repeat-penalty with inline reset controls |
| `InferenceParametersForm.css` | Slider and range input styling |

## Tristate Semantics

| Value | Meaning |
|-------|---------|
| `undefined` | Inherit from server/global default |
| `null` | Explicitly clear (override with "no value") |
| `number` | Explicit numeric override |

<!-- module-docs:end -->
