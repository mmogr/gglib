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
- `window` — showing, hiding and positioning the panel

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

That last rule is why the main window is declared `"visible": false` in
`tauri.conf.json`: the decision is made once, before anything is drawn, instead
of showing a window at every login only to snatch it away.

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

The panel is also not anchored to the icon on Linux: with no click event there
is no rectangle to anchor to, and `rect` is `None`. Falling back to the cursor
would be worse than leaving placement to the window manager — under Wayland
`cursor_position` returns `(0, 0)` rather than an error, which would throw the
panel into the top-left corner of the screen. Wayland forbids client-side
window placement outright, so there is nothing to recover here.

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
