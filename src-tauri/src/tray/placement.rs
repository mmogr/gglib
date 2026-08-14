//! Where the tray panel appears, and the one place that knows platforms differ.
//!
//! Callers describe *what they know about the tray icon* — a rectangle from a
//! click, a point, or nothing — and this module turns that into a position by
//! whatever means the platform offers. Keeping the `#[cfg]` here means
//! [`super::window`], [`super::build`] and [`super::handlers`] need none of
//! their own.

use tauri::WebviewWindow;
#[cfg(not(target_os = "linux"))]
use tauri::{PhysicalPosition, Rect};

/// What the caller knows about where the tray icon is.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Anchor {
    /// The icon's rectangle, from a click event that carried one.
    ///
    /// Only the `muda` backend reports these; Linux's tray fires no click
    /// events, which is why the variant does not exist there.
    #[cfg(not(target_os = "linux"))]
    Rect(Rect),
    /// A screen point, from a `StatusNotifierItem` activation.
    ///
    /// The SNI spec defines these as "a hint to the item where to show eventual
    /// windows", which is exactly the tray icon's position. Linux-only for the
    /// mirror-image reason `Rect` is not.
    #[cfg(target_os = "linux")]
    Point { x: i32, y: i32 },
    /// Nothing was reported.
    ///
    /// A menu item was used rather than the icon, so there is no gesture to
    /// anchor to.
    Unknown,
}

/// Prepare the panel for placement, once, before it is ever shown.
///
/// Returns whether the platform can place the panel itself. `false` is a
/// normal outcome, not a failure: it means the compositor decides, which is
/// what happens on X11 and on Wayland compositors without
/// `zwlr_layer_shell_v1`.
///
/// Must be called on the main thread during setup — see [`super::layer_shell`]
/// for why the timing is not incidental.
#[cfg(target_os = "linux")]
#[must_use]
pub(crate) fn prepare(panel: &WebviewWindow) -> bool {
    super::layer_shell::prepare(panel)
}

/// Prepare the panel for placement, once, before it is ever shown.
///
/// macOS and Windows position windows on demand in [`place`], so there is
/// nothing to set up ahead of time.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub(crate) fn prepare(_panel: &WebviewWindow) -> bool {
    true
}

/// Position the panel for this anchor, if the anchor and platform allow it.
///
/// On Linux the anchoring was already applied by [`prepare`] and the panel is
/// pinned beside the tray, so there is nothing per-toggle to do. Elsewhere a
/// known rectangle is used to drop the panel directly beneath the icon.
pub(crate) fn place(panel: &WebviewWindow, anchor: Anchor) -> tauri::Result<()> {
    match anchor {
        #[cfg(not(target_os = "linux"))]
        Anchor::Rect(rect) => position_near(panel, rect),
        #[cfg(target_os = "linux")]
        Anchor::Point { x, y } => {
            super::layer_shell::anchor_at(panel, x, y);
            Ok(())
        }
        // Guessing is worse than leaving it: `cursor_position` reports (0, 0)
        // on Wayland rather than failing, which would fling the panel into the
        // corner of the screen. The startup anchor still applies.
        Anchor::Unknown => Ok(()),
    }
}

/// Place the panel under the tray icon, horizontally centred on it.
///
/// The `Rect` path only, so it compiles where that anchor exists. Wayland has
/// its own route in [`super::layer_shell`], and cannot honour `set_position`.
///
/// Clamped to the monitor's work area so an icon near the right-hand edge —
/// where menu bar items usually are — does not push the panel off-screen.
#[cfg(not(target_os = "linux"))]
fn position_near(panel: &WebviewWindow, anchor: Rect) -> tauri::Result<()> {
    let scale = panel.scale_factor().unwrap_or(1.0);
    let anchor_position: PhysicalPosition<f64> = anchor.position.to_physical(scale);
    let anchor_size = anchor.size.to_physical::<f64>(scale);
    let panel_size = panel.outer_size()?;

    let mut x = anchor_position.x + (anchor_size.width / 2.0) - (f64::from(panel_size.width) / 2.0);
    let mut y = anchor_position.y + anchor_size.height;

    if let Ok(Some(monitor)) = panel.current_monitor() {
        let area = monitor.size();
        let origin = monitor.position();

        let max_x = f64::from(origin.x) + f64::from(area.width) - f64::from(panel_size.width);
        x = x.clamp(f64::from(origin.x), max_x.max(f64::from(origin.x)));

        // On a bottom-of-screen tray (Windows' default), dropping down would
        // land the panel off the bottom edge; put it above the icon instead.
        let below_bottom =
            y + f64::from(panel_size.height) > f64::from(origin.y) + f64::from(area.height);
        if below_bottom {
            y = anchor_position.y - f64::from(panel_size.height);
        }
    }

    panel.set_position(PhysicalPosition::new(x, y))
}
