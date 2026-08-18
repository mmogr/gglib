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
| `InspectorHeader.tsx` | Model name, sync and trust badges, and the header actions |
| `InspectorFooter.tsx` | Action row: serve or stop, edit, save, cancel, delete, benchmark |
| `InspectorCapabilities.tsx` | gglib's own editable shaping flags, over `CAPABILITY_FLAGS` |
| `InspectorTags.tsx` | `TagChips` plus the add control, as one editable tag section |
| `InspectorModals.tsx` | The panel's modals in one place, including the llama-server-not-installed path |
| `InspectorEmptyState.tsx` | Placeholder shown when no model is selected |
| `InfoRow.tsx` | One label/value row, the unit the metadata grid is built from |
| `MetadataSection.tsx` | Groups `InfoRow`s under a heading |
| `SamplingProvenanceSection.tsx` | Each resolved sampling parameter and the layer that supplied it |

<!-- module-docs:end -->
