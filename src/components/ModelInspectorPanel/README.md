# ModelInspectorPanel

<!-- module-docs:start -->

Right-hand detail panel for viewing, editing, and serving a selected GGUF model. Manages metadata display, inline editing, tag management, inference default overrides, serve configuration, and model deletion. A model of the paired machine is shown instead by `FarModelInspector`, which changes nothing there.

## Architecture

```
ModelInspectorPanel
    ├── ModelMetadataGrid      ← read-only metadata display
    │     ├── ProjectorRow     ← link or unlink the projector that lets the model read images
    │     ├── ComponentsRow    ← link or unlink each file an image model draws with, by role
    │     └── SamplingProvenanceSection ← resolved sampling + which layer won, handed in by the panel
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
| `ModelInspectorPanel.tsx` | The panel itself: composes the sections below and owns the selected model |
| `FarModelInspector.tsx` | The paired machine's model, read-only: its detail from that machine through `ModelMetadataGrid` (no path, no projector or components row, no sampling section), the Vision chip when that machine's listing says it reads images (as its library row does), "Serving on" that machine, Chat and Load only where that machine's actions list them and disabled while its rows are away or stale, the `gglib chat <id> --remote` that does the same from a terminal, and a re-read after Load |
| `components/` | `ModelMetadataGrid`, `ProjectorRow`, `ComponentsRow`, `SamplingProvenanceSection`, `ModelEditForm`, `TagChips`, `TagAddInput`, `ServeModal`, `JinjaModeField`, `ReasoningSupport`, `DeleteModal`, `InspectorFooter` |
| `hooks/` | `useEditMode`, `useModelDetail`, `useSamplingExplanation`, `useServeModal`, `useDeleteModal`, `useServerActions`, `useRetagModel` |

<!-- module-docs:end -->
