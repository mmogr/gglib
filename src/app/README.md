# app

<!-- module-docs:start -->

The main window's shell: its addresses, its router, and what navigates or acts from outside React. Every screen is a place with an address, written in one file and read back in one function; the native menu reaches the router and the registries directly, with no component to fill a callback for it.

## Key Files

| File | Role |
|------|------|
| `paths.ts` | Every path the app navigates to (`paths.library()`, `paths.model(id)`, `paths.chat(id)` …), `inspectorSubject` reading the selection back from the address, the settings dialog as a search parameter over the current place (`settingsAt`, `settingsSection`, `withoutSettings`), and the intents the menu and `+ ▾` carry in the address (`libraryIntent`: focus the search for Hugging Face, add a GGUF file, open the Serve dialog) |
| `router.tsx` | The routes and the one router: the library as a layout route over `/library`, `/models/:id` and `/far/:id` so a pick does not remount it; `createAppRouter(layout)` after the daemon's token has left the address; `appRouter()` for navigation from outside React; a memory router for tests |
| `Shell.tsx` | The window: the sidebar (`src/sidebar/`) beside the place the address names, no header; under 720 px the sidebar is a drawer behind a menu button, closed by a move, the scrim or Escape |
| `menuBridge.ts` | The native menu's events, handled from outside React: navigation through the router, model actions on the model the address names, the menu told what is selected on every move |
| `confirm.ts` | The confirmation question as a store, askable from outside React and drawn by `ConfirmProvider` |
| `ChatRoute.tsx` | `/chat/:modelId`: a chat on this machine's model, keyed by the model, its port from the server registry or the start that opened it |
| `FarChatRoute.tsx` | `/chat/far/:id`: a chat on the paired machine's model, its machine fixed for the chat's life |
| `BenchmarkRoute.tsx` | `/diagnostics/benchmarks?models=…`: the benchmarks with the named models picked |
| `useChatSwitch.ts` | Moving an open chat to another model: served first if not running, the last pick wins, a left chat hands nothing on |
| `chatHandoff.ts` | What a chat hands to the chat it becomes across a navigation: the port and name of a model just served, the conversation and the draft over a switch |

<!-- module-docs:end -->
