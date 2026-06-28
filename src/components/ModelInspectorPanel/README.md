# ModelInspectorPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelInspectorPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelInspectorPanel-complexity.json)

<!-- module-docs:start -->

Right-hand detail panel for viewing, editing, and serving a selected GGUF model. Manages metadata display, inline editing, tag management, inference default overrides, serve configuration, and model deletion.

## Architecture

```
ModelInspectorPanel
    ├── ModelMetadataGrid      ← read-only metadata display
    ├── TagChips + TagAddInput ← tag management
    ├── InferenceParametersForm ← per-model inference defaults
    ├── InspectorActions       ← serve / edit / delete / benchmark
    ├── ServeModal             ← context, port, jinja, MTP options
    └── DeleteModal            ← confirmation dialog
```

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `components/` | `ModelMetadataGrid`, `ModelEditForm`, `TagChips`, `TagAddInput`, `ServeModal`, `DeleteModal`, `InspectorActions` |
| `hooks/` | `useEditMode`, `useModelDetail`, `useServeModal`, `useDeleteModal`, `useServerActions` |

<!-- module-docs:end -->
