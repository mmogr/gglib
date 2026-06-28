# ServerList

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ServerList-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ServerList-complexity.json)

<!-- module-docs:start -->

Renderable list of running llama-server instances with per-server expand/collapse for view-tab selection (Chat / Console), a stop button, and a server health indicator. Supports `compact` mode for embedding inside `RunsPopover`.

## Key Files

| File | Role |
|------|------|
| `ServerList.tsx` | Maps servers to collapsible cards; one-at-a-time expansion; tab selection per server |

Only one server card can be expanded at a time (`expandedServerId` state). The embedded `SidebarTabs` lets users navigate directly to a server's chat or console view.

<!-- module-docs:end -->
