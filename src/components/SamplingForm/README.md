# SamplingForm

<!-- module-docs:start -->

The sampling form: a tristate inference parameter form where each field can be `undefined` (inherit parent default), `null` (explicitly unset), or a concrete number (override). Shows reset buttons when a field is explicitly set and displays the effective inherited value as a placeholder hint.

## What an empty field inherits

The form renders at three different rungs of the resolution ladder (request → profile → per-model → global settings, with a model's recipe beneath them), so an empty field means something different on each. Beneath the recipe there is no floor (ADR 0022): a field nothing names is not sent, and llama.cpp's own default applies. The required `fallback` prop says which rung this surface edits:

| Surface | `fallback` | An empty field falls through to |
|---|---|---|
| Settings → Global Inference Parameter Defaults | `{ kind: 'upstream' }` | a model's recipe where it has one, llama.cpp's own defaults otherwise — the caption names llama.cpp's number, and a model's own answer shows on the `explain` surface |
| The inspector's Sampling and Reasoning rows (`compact`) | `{ kind: 'resolved', ownLayer: 'modelUserSet' }` | the global setting, or whatever the backend says |
| `inspector/LoadOptionsSheet` | `{ kind: 'resolved', ownLayer: 'request' }` | profile → model → global → the model's recipe, then llama.cpp's defaults |

On the `resolved` surfaces the caption, placeholder, and slider position all come from `GET /api/models/:id/explain`. Nothing here re-derives the ladder: `resolve_layers_with_sources` computes value and provenance in one pass so that no second implementation can explain a decision the resolution did not take.

`ownLayer` is what lets a caption stay quiet when the value on screen came from the very layer being edited — clearing that field will re-resolve to something the saved explanation cannot yet name.

## The compact form

The inspector's Sampling and Reasoning rows open the form in place with `compact`: only the fields of the row that are set, then "Add a value" listing the rest, and one caption over them (where an empty field's value comes from) in place of a caption per field.

## The one conditional control

Every sampling parameter here reaches the sampler on every model. `reasoning_effort` does not: it is a variable handed to a chat template, and a template that does not read it ignores it in silence. gglib measures which templates those are (ADR 0007) and the server deletes the key before sending on a model whose observed capabilities say `no`.

So the optional `capabilities` prop carries that answer, and it has **three** states rather than two:

| `capabilities.reasoningEffort` | The effort control |
|---|---|
| `'yes'` | offered, captioned "the template reads it" |
| `'unknown'` | offered, captioned "not yet observed — start the model to check" |
| `'no'` | **hidden**, replaced by a visible sentence saying gglib will remove the level |
| prop omitted | offered, captioned with the condition — the global-settings surface, which has no model |

`unknown` is the common answer, not an edge case: capabilities are read from `GET /props` while a model runs, so every model nobody has launched on this installation answers it. Hiding the control there would gate it on nearly the whole library, which is the mistake ADR 0007 decision 3 forbids the server to make. Omitting the prop is therefore safe by construction — nothing is ever hidden by default, only captioned differently.

The **budget** is not gated at all. llama.cpp enforces it sampler-side, so no template can veto it, and the field shows for every model.

## Key Files

| File | Role |
|------|------|
| `SamplingForm.tsx` | Sliders and number inputs for the sampling parameters, plus the reasoning group, with inline reset controls; `compact` draws only the set fields and "Add a value" |
| `ReasoningEffortField.tsx` | The effort select and the three answers about whether it applies |
| `fallbackCaption.ts` | What to say (and what number to show) under an empty field on each surface |
| `SamplingForm.css` | Slider and range input styling |

## Tristate Semantics

| Value | Meaning |
|-------|---------|
| `undefined` | Inherit from server/global default |
| `null` | Explicitly clear (override with "no value") |
| `number` | Explicit numeric override |

<!-- module-docs:end -->
