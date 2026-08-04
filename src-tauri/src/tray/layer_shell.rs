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
