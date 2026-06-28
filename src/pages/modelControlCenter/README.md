# modelControlCenter

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-pages-modelControlCenter-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-pages-modelControlCenter-complexity.json)

<!-- module-docs:start -->

Custom hooks for the Model Control Center page's filter state, panel layout, and native menu action wiring. Keeps the page component thin by delegating all stateful logic here.

## Key Files

| File | Role |
|------|------|
| `useMccFilters.ts` | Filter state; debounced API calls (300ms); client-side full-text search on top of server results; model addition refresh |
| `useMccLayout.ts` | Panel resize state via `usePanelResize`; persists panel widths across sessions |
| `useMccMenuActions.ts` | Registers macOS menu callbacks; wires destructive operations through `ConfirmContext` |

## Filter Data Flow

```
User changes filter ──► debounced 300ms ──► API call
                                               ▼
                                      setServerModels(results)
                                               ▼
                         client-side useMemo(searchQuery, serverModels)
                                               ▼
                                      filteredModels → UI
```

<!-- module-docs:end -->
