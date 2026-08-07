# InferenceParametersForm

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-InferenceParametersForm-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-InferenceParametersForm-complexity.json)

<!-- module-docs:start -->

Tristate inference parameter form where each field can be `undefined` (inherit parent default), `null` (explicitly unset), or a concrete number (override). Shows reset buttons when a field is explicitly set and displays the effective inherited value as a placeholder hint.

## What an empty field inherits

The form renders at three different rungs of the resolution ladder (request → profile → per-model → global settings → floor), so an empty field means something different on each. The required `fallback` prop says which rung this surface edits:

| Surface | `fallback` | An empty field falls through to |
|---|---|---|
| Settings → Global Inference Parameter Defaults | `{ kind: 'floor' }` | the hardcoded floor — the one place a constant is correct |
| `ModelEditForm` | `{ kind: 'resolved', ownLayer: 'modelUserSet' }` | the global setting, or whatever the backend says |
| `ServeModal` | `{ kind: 'resolved', ownLayer: 'request' }` | profile → model → global, then the floor |

On the `resolved` surfaces the caption, placeholder, and slider position all come from `GET /api/models/:id/explain`. Nothing here re-derives the ladder: `resolve_layers_with_sources` computes value and provenance in one pass so that no second implementation can explain a decision the resolution did not take.

`ownLayer` is what lets a caption stay quiet when the value on screen came from the very layer being edited — clearing that field will re-resolve to something the saved explanation cannot yet name.

## Key Files

| File | Role |
|------|------|
| `InferenceParametersForm.tsx` | Sliders and number inputs for the seven sampling parameters, with inline reset controls |
| `fallbackCaption.ts` | What to say (and what number to show) under an empty field on each surface |
| `InferenceParametersForm.css` | Slider and range input styling |

## Tristate Semantics

| Value | Meaning |
|-------|---------|
| `undefined` | Inherit from server/global default |
| `null` | Explicitly clear (override with "no value") |
| `number` | Explicit numeric override |

<!-- module-docs:end -->
