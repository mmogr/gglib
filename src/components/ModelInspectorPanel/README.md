# ModelInspectorPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelInspectorPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelInspectorPanel-complexity.json)

<!-- module-docs:start -->

Right-hand detail panel for viewing, editing, and serving a selected GGUF model. Manages metadata display, inline editing, tag management, inference default overrides, serve configuration, and model deletion.

## Architecture

```
ModelInspectorPanel
    ├── ModelMetadataGrid      ← read-only metadata display
    │     └── SamplingProvenanceSection ← resolved sampling + which layer won
    ├── TagChips + TagAddInput ← tag management
    ├── InferenceParametersForm ← per-model inference defaults
    ├── InspectorActions       ← serve / edit / delete / benchmark
    ├── ServeModal             ← context, port, jinja, MTP options
    └── DeleteModal            ← confirmation dialog
```

Read mode shows what a model's sampling parameters *resolve to* and which
layer supplied each; edit mode shows the model's own stored defaults, which
are one rung of that resolution.

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `components/` | `ModelMetadataGrid`, `SamplingProvenanceSection`, `ModelEditForm`, `TagChips`, `TagAddInput`, `ServeModal`, `DeleteModal`, `InspectorActions` |
| `hooks/` | `useEditMode`, `useModelDetail`, `useSamplingExplanation`, `useServeModal`, `useDeleteModal`, `useServerActions` |

<!-- module-docs:end -->
