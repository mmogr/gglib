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
| `ServeModal.tsx` | Options form: context override, custom port, Jinja template, MTP settings, inference params |
| `DeleteModal.tsx` | Confirmation dialog for permanent model removal |
| `InspectorActions.tsx` | Action button row: Serve, Edit, Delete, Benchmark |

<!-- module-docs:end -->
