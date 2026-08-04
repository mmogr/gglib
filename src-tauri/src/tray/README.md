<!-- module-docs:start -->

System tray icon, menu and popover panel.

The tray is what makes the proxy usable as a background service: with
`close_to_tray` and `proxy_autostart` set, gglib's window is incidental and the
tray is the whole interface. The icon reflects the **proxy**, not the
application — the app being open says nothing about whether anything is being
served, and an icon that lit up merely because gglib was running would answer
a question nobody asked.

# Module Structure

- `ids` — tray menu item ID constants
- `icon` — pure `MenuState` → icon/tooltip derivation, unit-tested without a running app
- `build` — tray icon, menu and click behaviour
- `handlers` — thin dispatch to `proxy_actions`, `window` and `lifecycle`
- `window` — showing and hiding the panel and the main window
- `placement` — where the panel appears; the only module that branches on platform
- `layer_shell` — Linux-only Wayland placement, loaded at runtime

State is applied by `sync`, which is called from `menu::state_sync::sync_all_state`
rather than directly, so the tray cannot fall out of step with the macOS
application menu.

Proxy actions go through [`crate::proxy_actions`] rather than emitting an event
for the frontend to act on, the way the application menu does. The tray is
reachable when no window is visible and during autostart before any webview
exists, so it cannot assume something is loaded and listening.

Quit goes through [`crate::lifecycle::request_shutdown`], the same entry point
Cmd+Q and window close use, so there is exactly one shutdown sequence however
the user asked to quit.

## What the panel depends on staying alive

The panel reaches the backend over HTTP, through the embedded Axum server the
main window uses. That server has to outlive the window: closing to the tray
must hide it and tear down nothing, or the panel is left holding a dead port
and every button on it fails. `lifecycle::request_shutdown` is therefore the
*only* thing that runs cleanup, and close-to-tray never calls it.

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

## Platform differences

What the tray backend actually supports, which is why the menu carries
everything:

| | macOS / Windows | Linux (AppIndicator) |
|---|---|---|
| Menu | yes | yes |
| Click events (`on_tray_icon_event`) | yes | **never fire** |
| `set_tooltip` | yes | **silent no-op** |
| `TrayIcon::rect` | yes | **always `None`** |

Two consequences shape this module.

**Every action is on the menu**, on every platform. The click gesture that
opens the panel is a shortcut, never the only route to a feature, because on
Linux there is no click gesture at all.

**The endpoint is a menu item, not just a tooltip.** `set_tooltip` does nothing
on Linux, so hover text alone would leave the port invisible on exactly the
platform where the tray is the whole interface. `icon::derive` produces one
string and `sync` sends it to both, so they cannot disagree.

## Where the panel appears

Three different mechanisms, one per session type, all behind `placement`:

| Session | Mechanism | Result |
|---|---|---|
| macOS / Windows | `set_position` from the click's `rect` | Directly under the icon |
| Wayland + `zwlr_layer_shell_v1` | `layer_shell`, anchored bottom-right | Beside the system tray |
| X11, or Wayland without the protocol | none | Wherever the compositor decides |

`place` is called with an [`placement::Anchor`] describing *what the caller knows*
— a rectangle, or nothing — rather than with coordinates, because what is
knowable differs per platform. Linux reports `Unknown`: no click event fires and
`rect` is always `None`.

Guessing is deliberately avoided. `cursor_position` looks like a fallback but
returns `(0, 0)` on Wayland rather than failing, so anchoring to it would fling
the panel into the corner of the screen.

Wayland forbids a client from placing its own toplevels, which is why the
Wayland path is not `set_position` at all: `zwlr_layer_shell_v1` positions a
surface by anchoring it to screen edges and pushing it away with margins. The
anchor is set once at startup rather than per-toggle, because the system tray
does not move.

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

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
<!-- module-table:end -->

</details>
