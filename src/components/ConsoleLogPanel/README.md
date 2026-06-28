# ConsoleLogPanel

![LOC](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConsoleLogPanel-loc.json)
![Complexity](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges/ts-components-ConsoleLogPanel-complexity.json)

<!-- module-docs:start -->

Terminal-style log viewer for live llama-server output. Parses ANSI escape codes for colour rendering, implements smart auto-scroll (follows tail unless the user has scrolled up), and provides copy-to-clipboard and clear controls.

## Key Files

| File | Role |
|------|------|
| `ConsoleLogPanel.tsx` | Log list with ANSI parsing (Anser library), auto-scroll logic, copy/clear actions |
| `ConsoleLogPanel.css` | Terminal appearance, ANSI colour classes, monospace font |

Auto-scroll disables when the user scrolls up and re-enables automatically when they return to within 40px of the bottom.

<!-- module-docs:end -->
