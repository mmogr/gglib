# app

<!-- module-docs:start -->

The main window's shell: its addresses, its router, and what navigates or acts from outside React. Every screen is a place with an address, written in one file and read back in one function; the native menu reaches the router and the registries directly, with no component to fill a callback for it.

## Key Files

| File | Role |
|------|------|
| `paths.ts` | Every path the app navigates to (`paths.library()`, `paths.model(id)`, `paths.chat(id)` for a conversation, `paths.newChat({ model })`, `paths.devices()` …), `inspectorSubject` reading the selection back from the address (the path in the library, `?inspect=model:<id>` beside the endpoint) and `inspectorClosed` where × goes, `chatAddress` reading the chat back, the settings place's six sections (`paths.settings(section)`, `settingsSection` reading a segment back), and the intents the menu and `+ ▾` carry in the address (`libraryIntent`: focus the search for Hugging Face, add a GGUF file, open the inspector's Load with options sheet; `paths.endpointConnect()`, the endpoint at its Connect a client block) |
| `router.tsx` | The routes and the one router: the library as a layout route over `/library`, `/models/:id` and `/far/:id` so a pick does not remount it, `/endpoint`, `/devices`, and `/settings/:section` with `/settings` going to General; `createAppRouter(layout)` after the daemon's token has left the address; `appRouter()` for navigation from outside React; a memory router for tests |
| `Shell.tsx` | The window: the sidebar (`src/sidebar/`) beside the place the address names, no header; under 720 px the sidebar is a drawer behind a menu button, closed by a move, the scrim or Escape |
| `menuBridge.ts` | The native menus' events, handled from outside React: `menu:navigate`'s place ids turned into addresses (⌘1–⌘4 the library, the endpoint, the devices and the latest chat else an empty one; Settings…, the tray's too; Diagnostics; New Chat; Get Models…; Add a GGUF File…; Connect a Client…), the Model menu's Load, Load with Options…, Unload and Remove from Library… on the model the inspector names in the inspector's words, Hide Inspector as its ×, Copy's URL to the clipboard, the menu told what is selected on every move; where no menu bar holds New Chat (off a Mac, or a browser on one) its key is the window's |
| `confirm.ts` | The confirmation question as a store, askable from outside React and drawn by `ConfirmProvider` |
| `useOfferOutcomes.ts` | Mounted at the app's root: says a device paired, or that a code expired, stopped working early, or went when sharing stopped, on whatever place is open; reads the status again after an expiry so Invite comes back. The app's own Stop sharing and a Forget are said by nobody |
| `ChatRoute.tsx` | `/chat/:id` and `/chat/new?model=`/`?far=`: a conversation on the model it names (else the last used, the default, the first loaded), keyed by the model so a first send's replaced address keeps the place, its port from the server registry or the start that opened it |
| `PairedChat.tsx` | A conversation kept here on the paired machine's model, its machine fixed for the chat's life |
| `FarChatRoute.tsx` | `/chat/far/:id`: one of the paired machine's own chats, read and carried on, never changed; no port |
| `chatLoading.tsx` | What a chat route shows while it learns which chat to open, and when the list of chats cannot be read |
| `openChat.ts` | "Chat" on a model: its latest conversation, else an empty chat on it, handed the port a start or a resident slot answers on (the inspector, the library, the endpoint's Loaded rows, the menu) |
| `BenchmarkRoute.tsx` | `/diagnostics/benchmarks?models=…`: the benchmarks with the named models picked |
| `useChatSwitch.ts` | Moving an open chat to another model: served first if not running, the last pick wins, a left chat hands nothing on |
| `chatHandoff.ts` | What a chat hands to the chat it becomes across a navigation: the model to open on and its port and name, and the draft over a switch |

## Words this folder says that Rust does not

"Could not load <name>: <the daemon's words>" (the menu's Load refused); the
menu's Remove from Library… asks in the inspector's words ("Remove <name>
from the library?", "The file stays on disk.", "Remove", "Could not remove
<name>: …"). The menus' own words are Rust's, in `src-tauri/src/menu/` and
`src-tauri/src/tray/`.

<!-- module-docs:end -->
