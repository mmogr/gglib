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

## Platform differences

Linux's AppIndicator delivers no click events, so `on_tray_icon_event` never
fires there and the panel opens from the "Proxy Panel" menu item instead. Every
action is on the menu on all platforms for exactly this reason — the click
gesture is a shortcut, never the only route to a feature.

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
