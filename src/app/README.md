# app

<!-- module-docs:start -->

The main window's shell: its addresses, its router, and what navigates or acts from outside React. Every screen is a place with an address, written in one file and read back in one function; the native menu reaches the router and the registries directly, with no component to fill a callback for it.

## Key Files

| File | Role |
|------|------|
| `paths.ts` | Every path the app navigates to (`paths.library()`, `paths.model(id)`, `paths.chat(id)` for a conversation, `paths.newChat({ model })`, `paths.devices()` …), `inspectorSubject` reading the selection back from the address (the path in the library, `?inspect=model:<id>` beside the endpoint) and `inspectorClosed` where × goes, `chatAddress` reading the chat back, the settings place's six sections (`paths.settings(section)`, `settingsSection` reading a segment back), and the intents the menu and `+ ▾` carry in the address (`libraryIntent`: focus the search for Hugging Face, add a GGUF file, open the inspector's Load with options sheet; `paths.endpointConnect()`, the endpoint at its Connect a client block) |
| `router.tsx` | The routes and the one router: the library as a layout route over `/library`, `/models/:id` and `/far/:id` so a pick does not remount it, `/endpoint`, `/devices`, and `/settings/:section` with `/settings` going to General; `createAppRouter(layout)` after the daemon's token has left the address; `appRouter()` for navigation from outside React; a memory router for tests |
| `SetupGate.tsx` | Setup as a gate above the outlet: the setup place while the daemon says setup is not done, the shell once it is or once Open gglib writes it; a status that cannot be read opens the shell |
| `Shell.tsx` | The window: the sidebar (`src/sidebar/`) beside the place the address names, no header; under 720 px the sidebar is a drawer behind a menu button, closed by a move, the scrim or Escape |
| `menuBridge.ts` | The native menus' events, handled from outside React: `menu:navigate`'s place ids turned into addresses (⌘1–⌘4 the library, the endpoint, the devices and the latest chat else an empty one; Settings…, the tray's too; Diagnostics; New Chat; Get Models…; Add a GGUF File…; Connect a Client…), the Model menu's Load, Load with Options…, Unload and Remove from Library… on the model the inspector names in the inspector's words, Hide Inspector as its ×, Copy's URL to the clipboard, the menu told what is selected on every move; where no menu bar holds New Chat (off a Mac, or a browser on one) its key is the window's |
| `ServiceDown.tsx` | The service-not-running screen and `ServiceGate`, which puts it in the shell's place while the daemon does not answer its `/health` (never before its first answer) and gives the place back at its address when it does, the registries let go and read again as the tray does (`useDaemonRecovery`); in a browser, a daemon that refuses the tab's token is said in its own sentence instead (`tokenRefusal`, asked once each time the daemon is heard): one centred sentence beside the sidebar, dimmed and inert; on the desktop Start gglib (`startGglib`, the tray menu's `Daemon::restart`) with a refusal in the app's words, the port held by another program among them; in a browser "Lost gglib. Reconnecting…", the poll its retry |
| `confirm.ts` | The confirmation question as a store, askable from outside React and drawn by `ConfirmProvider` |
| `useOfferOutcomes.ts` | Mounted at the app's root: says a device paired, or that a code expired, stopped working early, or went when sharing stopped, on whatever place is open; reads the status again after an expiry so Invite comes back. The app's own Stop sharing and a Forget are said by nobody |
| `ChatRoute.tsx` | `/chat/:id` and `/chat/new?model=`/`?far=`: a conversation on the model it names (else the last used or the default while the library has it, else the first loaded, else the library), keyed by the model so a first send's replaced address keeps the place; it opens whether or not the model is running, since a send loads it; its port, from the server registry or the start that opened it, only names it for a title, and is kept once seen, so a model unloaded or crashed while the chat is open leaves it open, its composer saying so; the fallback is not decided before the server list's and the library's first reads land (`useServerListHeard`, `useModelLibrary`); the composer's Show <model>, Show log, Load it again and a paired model's new chat act from here |
| `PairedChat.tsx` | A conversation kept here on the paired machine's model, its machine fixed for the chat's life |
| `FarChatRoute.tsx` | `/chat/far/:id`: one of the paired machine's own chats, read and carried on, never changed; no port |
| `chatLoading.tsx` | What a chat route shows while it learns which chat to open, and when the list of chats cannot be read |
| `openChat.ts` | "Chat" on a model: its latest conversation, else an empty chat on it, handed the port a start or a resident slot answers on (the inspector, the library, the endpoint's Loaded rows, the menu) |
| `useChatSwitch.ts` | Moving an open chat to another model: saved as the conversation's model (`PUT /api/conversations/{id}`), nothing started, the last pick wins, a left chat hands nothing on; and Load it again, which starts the chat's own model after it was unloaded |
| `chatHandoff.ts` | What a chat hands to the chat it becomes across a navigation: the model to open on and its port and name, and the draft over a switch |

## Words this folder says that Rust does not

"Didn't load: <the daemon's words>" (the menu's Load refused, the inspector's words); the
menu's Remove from Library… asks in the inspector's words ("Remove <name>
from the library?", "The file stays on disk.", "Remove", "Could not remove
<name>: …"). The menus' own words are Rust's, in `src-tauri/src/menu/` and
`src-tauri/src/tray/`.

<!-- module-docs:end -->
