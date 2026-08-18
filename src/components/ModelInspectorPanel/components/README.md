# components

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelInspectorPanel-components-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelInspectorPanel-components-complexity.json)

<!-- module-docs:start -->

Presentational sub-components for the model inspector panel, each scoped to a single responsibility.

## Key Files

| File | Role |
|------|------|
| `ModelMetadataGrid.tsx` | Read-only grid: size, architecture, quantization, context window, path, HF link |
| `ModelEditForm.tsx` | Editable quantization label, file path, and inline `InferenceParametersForm` |
| `TagChips.tsx` | Tag pill list with individual remove buttons |
| `TagAddInput.tsx` | Controlled text input for adding new tags (submit on Enter) |
| `ServeModal.tsx` | Options form: context override, custom port, Jinja mode, MTP settings, inference params |
| `JinjaModeField.tsx` | Off / On / Defer as three options, because a launch has three states and a checkbox held two |
| `ReasoningSupport.tsx` | Whether this model's template reads `reasoning_effort`, and a re-measurement when the answer is stale |
| `DeleteModal.tsx` | Confirmation dialog for permanent model removal |

<!-- module-docs:end -->
