# hooks

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelInspectorPanel-hooks-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ModelInspectorPanel-hooks-complexity.json)

<!-- module-docs:start -->

Custom hooks encapsulating stateful logic for the model inspector panel.

## Key Files

| File | Role |
|------|------|
| `useEditMode.ts` | Tracks edit mode; captures pending edits for quantization, file path, inference defaults |
| `useModelDetail.ts` | Fetches extended model detail (GGUF metadata, tags); exposes refresh |
| `useServeModal.ts` | Serve modal open/close state and all serve option values |
| `useDeleteModal.ts` | Delete confirmation modal state |
| `useServerActions.ts` | Orchestrates `serveModel()` / `stopServer()` calls with error boundaries |

<!-- module-docs:end -->
