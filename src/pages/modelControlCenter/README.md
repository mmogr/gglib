# modelControlCenter

<!-- module-docs:start -->

Custom hooks for the Model Control Center page's filter state and panel layout. Which screen is open is the address (`app/router`), and the native menu acts through `app/menuBridge`, so neither is this page's to wire.

## Key Files

| File | Role |
|------|------|
| `useMccFilters.ts` | Filter state; debounced `listModels` calls (300ms) that carry every filter that is set, re-run on every read of the model registry; client-side full-text search on top of server results; model addition refresh |
| `useMccLayout.ts` | Panel resize state via `usePanelResize`; persists panel widths across sessions |

## Filter Data Flow

```
User changes filter ─────┐
Registry read again ─────┤  (refresh button, download done,
                         │   library event)
                         ▼
              debounced 300ms ──► API call
                                     ▼
                            setServerModels(results)
                                     ▼
               client-side useMemo(searchQuery, serverModels)
                                     ▼
                            filteredModels → UI
```

<!-- module-docs:end -->
