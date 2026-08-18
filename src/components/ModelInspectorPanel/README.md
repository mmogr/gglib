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
    ├── InspectorCapabilities  ← gglib's own editable shaping flags
    ├── ReasoningSupport       ← whether the template reads reasoning_effort
    ├── InspectorFooter        ← serve / edit / delete / benchmark
    ├── ServeModal             ← context, port, jinja mode, MTP options
    └── DeleteModal            ← confirmation dialog
```

Read mode shows what a model's sampling parameters *resolve to* and which
layer supplied each; edit mode shows the model's own stored defaults, which
are one rung of that resolution.

`ReasoningSupport` sits beside `InspectorCapabilities` and is deliberately not
part of it. Those four flags are gglib's own, and an operator corrects them
when detection got it wrong. Template support is an observation of somebody
else's template, taken by the renderer that executes it — so the panel offers a
re-measurement rather than an override, and says "start the model to check"
when there is nothing running to read.

## Sub-directories

| Directory | Contents |
|-----------|----------|
| `components/` | `ModelMetadataGrid`, `SamplingProvenanceSection`, `ModelEditForm`, `TagChips`, `TagAddInput`, `ServeModal`, `JinjaModeField`, `ReasoningSupport`, `DeleteModal`, `InspectorFooter` |
| `hooks/` | `useEditMode`, `useModelDetail`, `useSamplingExplanation`, `useServeModal`, `useDeleteModal`, `useServerActions`, `useRetagModel` |

<!-- module-docs:end -->
