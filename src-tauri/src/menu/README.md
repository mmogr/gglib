<!-- module-docs:start -->

Native application menu for the `gglib` GUI.

Provides the macOS menu bar, with stateful items that reflect the current
application state (the endpoint, the model in the inspector). Other platforms
are given an empty menu.

gglib (About gglib · Settings… ⌘, · the OS items) · File (Get Models… ⌘D ·
Add a GGUF File… ⌘O · New Chat ⌘N) · Edit · View (Library ⌘1 · Endpoint ⌘2 ·
Devices ⌘3 · Chat ⌘4 · Diagnostics ⌥⌘D · Hide Inspector ⌘I · Full Screen) ·
Model (Load ⌘↩ · Load with Options… ⌥⌘↩ · Unload ⌘. · Remove from Library…
⌘⌫, all disabled with no model inspected, as is Hide Inspector) · Endpoint
(Turn On / Off ⌘P, disabled with no daemon · Copy Endpoint URL ⇧⌘C and
Connect a Client…, both only while the endpoint is on) · Window · Help (gglib
Documentation).

A place is one `menu:navigate` event carrying a place id
(`app::events::places`); the Model menu and Hide Inspector send events of
their own, which the frontend runs on the model its address names. Turn On /
Off and Copy Endpoint URL are done here, natively, by the same
`proxy_actions` the tray uses, so they work with no webview listening.

# Module Structure

- `ids` - Menu item ID constants for event handling
- `build` - Menu construction, one file per submenu: `app` (gglib), `file`,
  `view`, `model`, `endpoint`, `standard` (Edit, Window, Help)
- `handlers` - Menu event handling: each id to a place, an event, or a native
  action
- `state_sync` - Menu and tray state synchronization with app state

`mod.rs` holds `MenuState` and the two pure rules over it: `is_enabled` and
`label` (Turn On while off, Turn Off while on).

<!-- module-docs:end -->
