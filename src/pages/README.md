<!-- module-docs:start -->

# Pages Module

Top-level page components for the gglib GUI application.

## Architecture

Two entry points build two windows; the main one routes by address (`app/router`):

```text
main.tsx ──► App.tsx ──► router                tray-main.tsx ──► tray/TrayPanel.tsx
                │  /library /models/:id(/log)      (separate "tray" Tauri window:
                │  /far/:id /get/:owner/:name       endpoint, loaded, devices)
                │  /chat/:id  /chat/new  /chat/far/:id
                │  /diagnostics/benchmarks
                ▼
  places/library · places/chat · BenchmarkPage
              ▼
        components/
   (Shared UI Components)
```

## Pages

| Page | Description |
|------|-------------|
| [`BenchmarkPage.tsx`](BenchmarkPage.tsx) | Benchmark workflows: compare, perf, and sampling-parameter tune, at `/diagnostics/benchmarks` through `app/BenchmarkRoute` |

The main window's landing page, the library at `/library`, `/models/:id` (and its `/log`), `/far/:id` and `/get/:owner/:name`, is a place (`src/places/library/`), not a page, and so is a chat (`src/places/chat/`). A model's log is its inspector's, not a view of the chat.

## Design Principles

1. **Page as Composition Root** — Pages compose hooks and components
2. **Minimal Logic** — Business logic lives in hooks, not pages
3. **Responsive Layout** — Pages adapt to different screen sizes

<!-- module-docs:end -->
