# RunsPopover

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-RunsPopover-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-RunsPopover-complexity.json)

<!-- module-docs:start -->

Floating popover listing currently running llama-server instances with per-server stop buttons and quick navigation to each server's chat or console view. Auto-closes when the last server stops.

## Key Files

| File | Role |
|------|------|
| `RunsPopover.tsx` | Fixed-position popover wrapping `ServerList`; auto-close effect; refresh button |

When `servers.length` drops to 0, a `useEffect` triggers `onClose()` automatically so the popover doesn't linger as an empty panel.

<!-- module-docs:end -->
