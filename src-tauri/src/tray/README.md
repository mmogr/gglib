<!-- module-docs:start -->

System tray icon, menu and popover panel.

The tray is what makes gglib usable as a background service: with
`close_to_tray` and `proxy_autostart` set, gglib's window is incidental and the
tray is the whole interface.

## What the icon means

**Consumption, not existence.** Not the application — an icon that lit up
merely because a window was open would answer a question nobody asked — and not
a live daemon either, which is the trap the obvious reading falls into: nearly
every CLI command calls `ensure_daemon()` and leaves one running, so an icon lit
by that would be lit permanently and mean nothing.

What it reports is whether gglib is doing something to this machine right now:

| | |
|---|---|
| **Offline** | No daemon answering. gglib is not running here. |
| **Idle** | A daemon is up, serving nothing and holding nothing. |
| **Active** | The proxy is listening, **or** a model is resident. |

The `or` is the point. The runtime holds up to `SLOT_COUNT` models in VRAM, and
`gglib chat`, `gglib serve` and benchmarks all leave one there with the proxy
stopped. An icon that tracked only the proxy showed idle while tens of
gigabytes were spoken for — which is precisely the question a menu bar is good
at answering.

This was a proxy-only tray until the daemon consolidation moved runtime
ownership out of the desktop app. The rule did not change; what counts as
"exists" did.

# Module Structure

- `ids` — tray menu item ID constants
- `items` — the menu itself: order, labels, and the one enabled rule
- `icon` — the tray id, the decoded icons, and pure state → appearance derivation
- `confirm` — what a teardown would cost, and the dialogs that say so
- `handlers` — `dispatch(app, id)`, the single router every backend calls
- `build` — the `muda` backend (macOS, Windows)
- `linux` — the `StatusNotifierItem` backend (Linux)
- `window` — showing and hiding the panel and the main window
- `placement` — where the panel appears; the only module that branches on platform
- `layer_shell` — Linux-only Wayland placement, loaded at runtime

## Where the state comes from

A [`crate::daemon::DaemonSnapshot`], written by one polling task
(`daemon::watch`) and by nothing else. The tray does not track what it last did:
it used to, and a proxy started by `proxy_autostart`, by the CLI or by the
window left it wrong for the rest of the session.

`sync` is called from `menu::state_sync::sync_all_state` rather than directly,
so the tray cannot fall out of step with the macOS application menu — and only
when the snapshot actually changed, because a repaint re-decodes the icon on
macOS and makes `ksni` rebuild the whole menu over D-Bus on Linux.

Proxy actions go through [`crate::proxy_actions`] rather than emitting an event
for the frontend to act on, the way the application menu does. The tray is
reachable when no window is visible and during autostart before any webview
exists, so it cannot assume something is loaded and listening. Those actions
ask the watcher for an immediate poll rather than publishing what they expect
to be true, so there is exactly one writer and nothing to lose an update to.

## Quit, and the service

Quit goes through [`crate::lifecycle::request_shutdown`], the same entry point
Cmd+Q and window close use, so there is exactly one shutdown sequence however
the user asked to quit. What that shutdown *takes with it* is decided by
[`crate::daemon::Ownership`]: a daemon this app launched or hosts is stopped,
one that was already answering is left alone.

**Stop gglib Service** is the separate verb for ending a daemon without
quitting, and **Start gglib Service** the way back — reachable even when the
app came up with no daemon at all, which used to be a panic before any tray
existed. Both warn first, from `confirm`, and the warning is derived from the
snapshot: it names the port and the resident models actually at stake rather
than asserting a fixed sentence, which is how the old one came to claim that
quitting stopped a proxy it had long since stopped stopping.

## What the panel depends on staying alive

The panel reaches the daemon over HTTP, on the fixed loopback port every
surface uses. The daemon has to outlive the window: closing to the tray must
hide it and tear down nothing, or the panel is left holding a dead port and
every button on it fails. `lifecycle::request_shutdown` is therefore the *only*
thing that runs cleanup, and close-to-tray never calls it.

## Menu-bar-only mode on macOS

With `close_to_tray` on, closing the window also drops gglib out of the Dock —
[`crate::dock`] switches the activation policy to `Accessory` — and `show_main`
puts it back. Otherwise the app reads as one you merely hid, rather than the
background service it is.

An `Accessory` app leaves the **Cmd+Tab switcher** as well as the Dock; macOS
offers no way to have one without the other. So while hidden, the tray icon is
the only way back, and two rules follow from that:

- The Dock icon is restored in `window::show_main`, the single chokepoint every
  route back shares, rather than at each of its callers.
- gglib never starts hidden unless the tray was built successfully. A launch
  with no window, no Dock icon and no tray icon could only be recovered from a
  terminal, so `autostart::should_start_hidden` requires all three conditions
  and everything else shows the window.

The main window is nonetheless declared **visible**, and
`autostart::apply_initial_visibility` *hides* it for the login case rather than
showing it for every other one. That is the opposite of the obvious design and
it is load-bearing: a window created hidden and shown later never gets a correct
`xdg_surface` configure round-trip from KWin, leaving its server-side titlebar
buttons dead until a resize forces one. Declaring it hidden made every ordinary
launch look broken in order to tidy up the rare automatic one.

Hiding it in the setup hook costs nothing, because nothing has been drawn yet
either way. `tauri::app::setup` creates the config windows immediately before
calling that hook, both inside `build()`, and GTK only completes a queued map
once `run()` pumps the event loop — so the window is still unmapped and hiding
it cancels the map outright.

The tray panel keeps `"visible": false`, for an unrelated reason that is just as
load-bearing: layer-shell can only claim a window that has not been realized.

Only the Dock half is macOS-specific — [`crate::dock`] compiles to no-ops
elsewhere, because a taskbar entry belongs to a window rather than to the
process, so hiding the window has already removed it. Starting hidden at login
is not: `should_start_hidden` is platform-independent, and the login item
carries `--from-autostart` on all three platforms.

## Two backends, one menu

Tauri's tray goes through `libappindicator` on Linux, which registers its item
with `ItemIsMenu` and delivers no click events at all — `on_tray_icon_event`
never fires, `TrayIcon::rect` is always `None`, and `set_tooltip` is a silent
no-op. So Linux talks `StatusNotifierItem` directly instead, via `ksni`:

| | `build` (macOS, Windows) | `linux` (Linux) |
|---|---|---|
| Library | `muda` / Tauri tray | `ksni` over D-Bus |
| Left click | `on_tray_icon_event` | `Tray::activate(x, y)` |
| Icon position | `rect` from the event | the activation's coordinates |
| Icon format | `Image` | ARGB32 pixmap |

Only `mod.rs` names either one. Both expose the same `Handle`, `build` and
`sync`, so `AppState`, `main` and `menu::state_sync` hold a `tray::Tray` and
carry no `#[cfg]` at all.

What must not diverge is shared outright rather than by convention:

- **`items::ITEMS`** is the menu — order and labels — and `items::is_enabled`
  is the single rule for what is greyed out, including that the status header
  is never clickable. Both are pure and both are tested.
- **`handlers::dispatch(app, id)`** performs the action. `muda` adapts its
  `MenuEvent` to it in `build`; `ksni` calls it straight from an item callback.
  Either way the tray reaches `proxy_actions` and `lifecycle::request_shutdown`
  by the same path as the WebUI and the CLI.
- **`icon::derive`** produces one status string for the tooltip and the menu
  header, so they cannot disagree.

Both backends decode through `icon`, and both work in raw RGBA from there:
`ksni` wants ARGB32 in network byte order and Tauri hands back RGBA, so the
conversion is moving the alpha byte to the front of each pixel — no image
dependency for a byte swap.

**Every action is on the menu**, on both backends. The click gesture is a
shortcut, never the only route to a feature.

## Where the panel appears

Three different mechanisms, one per session type, all behind `placement`:

| Session | Mechanism | Result |
|---|---|---|
| macOS / Windows | `set_position` from the click's `rect` | Directly under the icon |
| Wayland + `zwlr_layer_shell_v1` | `layer_shell`, anchored to the activation point | Beside the icon |
| X11, or Wayland without the protocol | none | Wherever the compositor decides |

`place` takes an `Anchor` describing *what the caller knows*, and the vocabulary
itself differs per backend: `Rect` exists only where click events carry one,
`Point` only where SNI reports activation coordinates. A menu entry reports
`Unknown`, because no gesture located the icon, and the startup anchor stands.

Guessing is deliberately avoided. `cursor_position` looks like a fallback but
returns `(0, 0)` on Wayland rather than failing, so anchoring to it would fling
the panel into the corner of the screen.

Wayland forbids a client from placing its own toplevels, which is why the
Wayland path is not `set_position` at all: `zwlr_layer_shell_v1` positions a
surface by anchoring it to screen edges and pushing it away with margins.

A startup anchor puts the panel in the tray's corner before any coordinates are
known; `activate` then re-anchors it to the icon itself. Only the margins
change, so the surface is never re-initialised. `margins_for` converts a screen
point into distances from the two anchored edges, clamped so an icon near an
edge cannot push the panel off-screen — and floored so a panel larger than the
display cannot invert the clamp's bounds, which would panic.

`libgtk-layer-shell` is **loaded at runtime, not linked**. Linking would make it
a launch requirement for every Linux user, including X11 users who gain nothing
from it. Absent library or absent protocol — Mutter does not implement it —
means `prepare` reports `false` and the panel simply keeps the compositor's
placement. `gglib config check-deps` lists it as optional and names the right
package per distribution.

The timing is not incidental: `gtk_layer_init_for_window` has to run before the
window is realized, and the panel is declared `"visible": false`, so tao calls
`hide()` rather than `show_all()` on it and it stays unrealized until first
shown. That is why `prepare` is called from `setup_app` — on the main thread,
where GTK requires it — rather than on the first toggle.

macOS recolours template images for light and dark menu bars. Both icons are
pure black with the glyph carried entirely by the alpha channel, which is what
`icon_as_template` expects; a coloured icon would render as a dark smudge on a
dark menu bar.

That is also why there are two files for three states. The offline icon is the
idle ring with its alpha scaled down, derived at runtime in `icon` rather than
shipped: it is the same glyph, so a third asset could only drift from it, and
because the shape lives entirely in the alpha channel, fading that channel is
exactly "draw the same thing fainter" on all three platforms — the same
reasoning that keeps the ARGB byte swap dependency-free.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
