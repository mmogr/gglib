# hooks

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
| `useInspectorModals.ts` | Modal state the panel opens reactively, chiefly the llama-server install prompt after a failed start |
| `useSamplingExplanation.ts` | Fetches the resolved sampling explanation for the selected model |
| `useHfDownload.ts` | Queues a HuggingFace preview's download; the button is off when the queue snapshot says `full` |
| `useRetagModel.ts` | Re-derives capability tags from the GGUF: confirm on a destructive rebuild, then toast and reload |

<!-- module-docs:end -->
