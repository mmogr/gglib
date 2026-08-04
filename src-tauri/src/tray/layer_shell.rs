//! Wayland layer-shell placement for the tray panel.
//!
//! Wayland gives a client no say in where its own toplevel windows go, so the
//! panel that drops out of the menu bar on macOS lands wherever the compositor
//! feels like on Linux. `zwlr_layer_shell_v1` is the exception: a surface in a
//! layer is positioned by anchoring it to screen edges and pushing it away with
//! margins, which is enough to put the panel beside the system tray.
//!
//! # Why the library is loaded rather than linked
//!
//! `gtk-layer-shell` is not installed everywhere, and linking it would make it
//! a hard requirement — every Linux user would need it to *launch*, including
//! the ones on X11 who gain nothing from it. Loading it at runtime keeps the
//! feature genuinely optional: no library, or a compositor without the
//! protocol, and [`prepare`] simply reports failure and the panel keeps its
//! previous behaviour.
//!
//! # Why this happens once, during setup
//!
//! `gtk_layer_init_for_window` must run before the window is realized. The
//! panel is declared `"visible": false`, so tao calls `hide()` rather than
//! `show_all()` on it and it stays unrealized until first shown — which is why
//! [`prepare`] is called from `setup_app` and not on the first toggle. GTK is
//! also main-thread-only, and `setup_app` is on the main thread.

use std::ffi::c_int;
use std::sync::OnceLock;

use gtk::glib::translate::ToGlibPtr;
use libloading::Library;
use tauri::WebviewWindow;
use tracing::{debug, info};

/// Opaque `GtkWindow *`.
type WindowPtr = *mut std::ffi::c_void;

const LAYER_OVERLAY: c_int = 3;
const EDGE_RIGHT: c_int = 1;
const EDGE_BOTTOM: c_int = 3;
/// The panel has buttons, and the default of "no keyboard" would make it inert.
const KEYBOARD_ON_DEMAND: c_int = 2;

/// Gap from the screen edges, in logical pixels, so the panel does not sit
/// flush against the taskbar it is anchored beside.
const EDGE_MARGIN: c_int = 8;

/// The five entry points needed, resolved once from `libgtk-layer-shell`.
struct Api {
    is_supported: unsafe extern "C" fn() -> c_int,
    init_for_window: unsafe extern "C" fn(WindowPtr),
    set_layer: unsafe extern "C" fn(WindowPtr, c_int),
    set_anchor: unsafe extern "C" fn(WindowPtr, c_int, c_int),
    set_margin: unsafe extern "C" fn(WindowPtr, c_int, c_int),
    set_keyboard_mode: unsafe extern "C" fn(WindowPtr, c_int),
    _lib: Library,
}

impl Api {
    /// Resolve the symbols, or `None` if the library is not installed.
    unsafe fn load() -> Option<Self> {
        // SAFETY: the symbols below are the documented gtk-layer-shell C API
        // and are matched to their real signatures.
        unsafe {
            let lib = Library::new("libgtk-layer-shell.so.0").ok()?;
            Some(Self {
                is_supported: *lib.get(b"gtk_layer_is_supported\0").ok()?,
                init_for_window: *lib.get(b"gtk_layer_init_for_window\0").ok()?,
                set_layer: *lib.get(b"gtk_layer_set_layer\0").ok()?,
                set_anchor: *lib.get(b"gtk_layer_set_anchor\0").ok()?,
                set_margin: *lib.get(b"gtk_layer_set_margin\0").ok()?,
                set_keyboard_mode: *lib.get(b"gtk_layer_set_keyboard_mode\0").ok()?,
                _lib: lib,
            })
        }
    }
}

/// Loaded once; `None` means the library is missing, which is not an error.
static API: OnceLock<Option<Api>> = OnceLock::new();

/// The resolved API, if the compositor also implements the protocol.
///
/// Mutter does not implement `zwlr_layer_shell_v1` at all, and neither does
/// X11, so the support check is as necessary as the library check.
fn api() -> Option<&'static Api> {
    // SAFETY: `load` only resolves symbols; `is_supported` takes no arguments
    // and touches no window state.
    let api = API.get_or_init(|| unsafe { Api::load() }).as_ref()?;

    unsafe { (api.is_supported)() != 0 }.then_some(api)
}

/// Make the panel a layer surface anchored beside the system tray.
///
/// Returns whether it took effect, so the caller can report which placement the
/// session actually got. Anchoring is set here rather than per-toggle because
/// the tray does not move: one anchor at startup is the whole of the placement
/// until real icon coordinates are available.
///
/// Bottom-right matches the default panel position on KDE and most desktops.
#[must_use]
pub fn prepare(panel: &WebviewWindow) -> bool {
    let Some(api) = api() else {
        debug!("gtk-layer-shell unavailable; leaving panel placement to the compositor");
        return false;
    };

    let Ok(window) = panel.gtk_window() else {
        debug!("Tray panel has no GTK window; skipping layer-shell setup");
        return false;
    };
    // A GtkApplicationWindow *is* a GtkWindow — GObject lays the parent struct
    // out first — so the pointer is passed straight through. The stash borrows
    // nothing for a GObject, so it stays valid as long as `window` does.
    let raw: *mut gtk::ffi::GtkApplicationWindow = window.to_glib_none().0;
    let ptr: WindowPtr = raw.cast();

    // SAFETY: `ptr` is a live GtkWindow owned by the panel, and this runs on
    // the main thread before the window is realized, as the API requires.
    unsafe {
        (api.init_for_window)(ptr);
        (api.set_layer)(ptr, LAYER_OVERLAY);
        (api.set_keyboard_mode)(ptr, KEYBOARD_ON_DEMAND);

        for edge in [EDGE_BOTTOM, EDGE_RIGHT] {
            (api.set_anchor)(ptr, edge, 1);
            (api.set_margin)(ptr, edge, EDGE_MARGIN);
        }
    }

    info!("Tray panel placed with layer-shell");
    true
}

/// Margins from the bottom and right edges that put a panel of `panel` size at
/// screen point `(x, y)` on a `screen`-sized display.
///
/// Layer-shell positions by edge and margin rather than by coordinate, so the
/// icon's position has to be expressed as a distance from the edges we anchored
/// to. Bottom-right for both, because that is where every default desktop panel
/// puts its system tray.
///
/// Pure, and clamped: an icon close to an edge would otherwise push part of the
/// panel off-screen, and a negative margin is not a thing layer-shell can honour.
fn margins_for(
    (x, y): (i32, i32),
    (screen_w, screen_h): (i32, i32),
    (panel_w, panel_h): (i32, i32),
) -> (i32, i32) {
    // The largest margin that still leaves the panel fully on screen. Floored
    // at EDGE_MARGIN because a panel bigger than the display would otherwise
    // give an upper bound below the lower one, which `clamp` panics on.
    let widest = (screen_w - panel_w).max(EDGE_MARGIN);
    let tallest = (screen_h - panel_h).max(EDGE_MARGIN);

    // Centre the panel on the icon horizontally, then measure back from the
    // right edge, which is the edge we are anchored to.
    let left = x - panel_w / 2;
    let right_margin = (screen_w - left - panel_w).clamp(EDGE_MARGIN, widest);

    // Sit above the icon rather than under it: a tray at the bottom of the
    // screen has no room below, which is the common case on KDE and Windows.
    let bottom_margin = (screen_h - y).clamp(EDGE_MARGIN, tallest);

    (bottom_margin, right_margin)
}

/// Re-anchor the panel so it sits beside the icon that was just clicked.
///
/// Unlike [`prepare`] this runs per activation, because it is the only moment
/// the icon's position is known. The anchors themselves do not change — only
/// the margins — so the surface is not re-initialised.
pub fn anchor_at(panel: &WebviewWindow, x: i32, y: i32) {
    let Some(api) = api() else { return };

    let Ok(window) = panel.gtk_window() else {
        return;
    };
    let raw: *mut gtk::ffi::GtkApplicationWindow = window.to_glib_none().0;
    let ptr: WindowPtr = raw.cast();

    let Some(screen) = panel.current_monitor().ok().flatten() else {
        return;
    };
    let scale = panel.scale_factor().unwrap_or(1.0);
    let size = screen.size().to_logical::<f64>(scale);
    let panel_size = panel.outer_size().map(|s| s.to_logical::<f64>(scale));
    let Ok(panel_size) = panel_size else { return };

    #[allow(clippy::cast_possible_truncation)]
    let (bottom, right) = margins_for(
        (x, y),
        (size.width as i32, size.height as i32),
        (panel_size.width as i32, panel_size.height as i32),
    );

    // SAFETY: `ptr` is a live GtkWindow already initialised for layer-shell by
    // `prepare`; setting a margin on a mapped layer surface is supported.
    unsafe {
        (api.set_margin)(ptr, EDGE_BOTTOM, bottom);
        (api.set_margin)(ptr, EDGE_RIGHT, right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: (i32, i32) = (1920, 1080);
    const PANEL: (i32, i32) = (360, 480);

    /// A tray icon near the bottom-right, which is where it normally is.
    #[test]
    fn an_icon_in_the_usual_corner_puts_the_panel_beside_it() {
        let (bottom, right) = margins_for((1850, 1050), SCREEN, PANEL);

        // Panel centred on the icon: 1920 - (1850 - 180) - 360 = -110, clamped.
        assert_eq!(right, EDGE_MARGIN);
        assert_eq!(bottom, SCREEN.1 - 1050);
    }

    /// An icon at the far right must not push the panel off-screen.
    #[test]
    fn an_icon_at_the_screen_edge_keeps_the_panel_on_screen() {
        let (_, right) = margins_for((1919, 1050), SCREEN, PANEL);

        assert!(right >= EDGE_MARGIN, "margin {right} would clip the panel");
        assert!(
            right <= SCREEN.0 - PANEL.0,
            "margin {right} would push the panel off the left"
        );
    }

    /// A left-hand tray — some people move the panel — must still resolve to a
    /// margin that keeps the whole panel visible.
    #[test]
    fn an_icon_on_the_left_still_yields_a_usable_margin() {
        let (_, right) = margins_for((20, 1050), SCREEN, PANEL);

        assert!(right <= SCREEN.0 - PANEL.0);
        assert!(right >= 0);
    }

    /// A top-of-screen tray, as on GNOME-style layouts: the panel hangs from
    /// the top, so the bottom margin is large but still leaves it on screen.
    #[test]
    fn an_icon_at_the_top_anchors_far_from_the_bottom() {
        let (bottom, _) = margins_for((1850, 10), SCREEN, PANEL);

        assert_eq!(bottom, SCREEN.1 - PANEL.1);
    }

    /// Margins are never negative, whatever the inputs — layer-shell cannot
    /// honour one, and nor may the clamp itself panic.
    #[test]
    fn margins_stay_valid_for_any_point() {
        for point in [(0, 0), (1919, 1079), (960, 540), (-50, -50), (9999, 9999)] {
            let (bottom, right) = margins_for(point, SCREEN, PANEL);
            assert!(bottom >= 0, "bottom {bottom} at {point:?}");
            assert!(right >= 0, "right {right} at {point:?}");
        }
    }

    /// A panel larger than the display would invert the clamp's bounds, which
    /// panics. Contrived, but a 4K panel on a small external screen is not.
    #[test]
    fn a_panel_larger_than_the_screen_does_not_panic() {
        let (bottom, right) = margins_for((100, 100), (320, 240), PANEL);

        assert_eq!(bottom, EDGE_MARGIN);
        assert_eq!(right, EDGE_MARGIN);
    }
}
