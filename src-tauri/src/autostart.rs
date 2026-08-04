//! Always-on proxy startup and OS login-item registration.
//!
//! The proxy has always been a feature you switch on from inside the app,
//! which makes it awkward as an endpoint for other clients: something has to
//! be open and something has to have been clicked. The two settings handled
//! here — `proxy_autostart` and `start_at_login` — turn it into a background
//! service instead, and together with `close_to_tray` mean the endpoint is
//! simply there.
//!
//! Both operations are best-effort. Neither is worth failing a launch over:
//! an app that refuses to open because a login item could not be registered
//! is strictly worse than one that opens and says so in the log.

use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;
use tracing::{error, info};

use crate::app::AppState;
use crate::proxy_actions;
use crate::{dock, tray};

/// Argument the login item passes, marking a launch as automatic.
///
/// Registered with the autostart plugin at build time and read back out of
/// `std::env::args`. macOS's `LaunchAgent` writes it into the plist's
/// `ProgramArguments` alongside the executable path, and the equivalent happens
/// on Windows and Linux.
pub const LOGIN_ITEM_FLAG: &str = "--from-autostart";

/// Whether this process was started by the OS login item rather than by hand.
fn launched_by_login_item() -> bool {
    std::env::args().any(|arg| arg == LOGIN_ITEM_FLAG)
}

/// Whether this launch should go straight to the menu bar, with no window and
/// no Dock icon.
///
/// Pure so the truth table can be tested without a running app — including the
/// case that matters most, which is every input that must still yield a window.
///
/// `tray_available` is not a formality. Building the tray is allowed to fail
/// without stopping the launch, and starting hidden without one would leave an
/// app with no window, no Dock icon and nothing to click: unreachable short of
/// killing it from a terminal. Requiring a tray means the worst outcome is a
/// window someone did not want, which they can close.
pub const fn should_start_hidden(
    launched_by_login_item: bool,
    close_to_tray: Option<bool>,
    tray_available: bool,
) -> bool {
    launched_by_login_item && matches!(close_to_tray, Some(true)) && tray_available
}

/// Hide the main window when this launch belongs in the menu bar.
///
/// Hides rather than shows, which is the opposite of the obvious design and is
/// load-bearing on Wayland. A window created hidden and shown later never gets
/// a correct `xdg_surface` configure round-trip from KWin, so its server-side
/// titlebar buttons are dead until a resize forces one — the app looks broken
/// on every ordinary launch to buy tidiness on the rare automatic one.
///
/// Declaring it visible and hiding it here costs nothing, because nothing has
/// been drawn yet either way: `tauri::app::setup` creates the config windows
/// immediately before calling this hook, both inside `build()`, and GTK only
/// completes a queued map once `run()` pumps the event loop. So the window is
/// still unmapped at this point and hiding it cancels the map outright.
pub async fn apply_initial_visibility(app: &AppHandle, tray_available: bool) {
    let state = app.state::<AppState>();

    let close_to_tray = match state.core.settings().get().await {
        Ok(settings) => settings.close_to_tray,
        Err(e) => {
            // Fail visible, deliberately. A window shown to someone who wanted
            // it hidden is a papercut they can fix with one click; a hidden
            // window is only recoverable through the tray, and this branch runs
            // precisely when the app is least healthy.
            error!(error = %e, "Could not read close_to_tray; showing the window");
            None
        }
    };

    if !should_start_hidden(launched_by_login_item(), close_to_tray, tray_available) {
        return;
    }

    dock::hide(app);

    if let Some(main) = app.get_webview_window(tray::window::MAIN_LABEL)
        && let Err(e) = main.hide()
    {
        // Left visible on failure, deliberately: a window nobody asked for is a
        // papercut, and this is the branch where the alternative is an app with
        // no window and no way to tell something went wrong.
        error!(error = %e, "Could not hide the main window; leaving it visible");
        return;
    }

    info!("Launched at login with close-to-tray - starting in the menu bar");
}

/// Bring the proxy up when `proxy_autostart` is set, and register or
/// unregister the login item to match `start_at_login`.
///
/// Runs after the window exists so a slow or failing start delays nothing the
/// user is waiting on.
pub async fn apply(app: &AppHandle) {
    let state = app.state::<AppState>();

    let settings = match state.core.settings().get().await {
        Ok(settings) => settings,
        Err(e) => {
            error!(error = %e, "Could not read settings; skipping autostart");
            return;
        }
    };

    apply_login_item(app, settings.start_at_login == Some(true));

    if settings.proxy_autostart == Some(true) {
        start_proxy(app).await;
    }
}

/// Start the proxy and publish the result to the rest of the app.
///
/// Uses `ensure_running`, which is idempotent and reads the saved
/// `proxy_port`: a user with a standing `gglib proxy` on that port gets their
/// own process left alone rather than a bind conflict at every launch.
async fn start_proxy(app: &AppHandle) {
    // Shared with the tray, so an automatic start and a manual one leave the
    // app in exactly the same state.
    match proxy_actions::start(app).await {
        Ok(port) => info!(port, "Proxy started automatically"),
        Err(e) => {
            // A failure here is recoverable: the user can still start the
            // proxy by hand, and the most likely cause is a port already
            // taken by something they started themselves.
            error!(error = %e, "Proxy autostart failed; continuing without it");
        }
    }
}

/// Register or unregister the OS login item, logging rather than failing.
///
/// Called on every launch rather than only when the setting changes, so a
/// login item removed outside gglib is restored instead of leaving the stored
/// value describing something that is no longer true.
fn apply_login_item(app: &AppHandle, enabled: bool) {
    let manager = app.autolaunch();

    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };

    match result {
        Ok(()) => info!(enabled, "Login item synchronised"),
        Err(e) => error!(error = %e, enabled, "Could not update login item"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one case that hides: the OS started us, and the user asked for
    /// gglib to live in the menu bar.
    #[test]
    fn a_login_launch_with_close_to_tray_starts_hidden() {
        assert!(should_start_hidden(true, Some(true), true));
    }

    /// Launching by hand always shows, whatever close-to-tray says. Someone
    /// who just double-clicked the app wants to see it.
    #[test]
    fn opening_the_app_yourself_always_shows_it() {
        assert!(!should_start_hidden(false, Some(true), true));
        assert!(!should_start_hidden(false, Some(false), true));
        assert!(!should_start_hidden(false, None, true));
    }

    /// Close-to-tray is the opt-in. Without it a login launch behaves the way
    /// it always has, so nobody who has not asked for this sees a change.
    #[test]
    fn a_login_launch_without_close_to_tray_still_shows() {
        assert!(!should_start_hidden(true, Some(false), true));
        assert!(!should_start_hidden(true, None, true));
    }

    /// No tray, no hiding — the guard against the one unrecoverable state,
    /// where nothing is on screen and there is nothing to click either.
    #[test]
    fn nothing_hides_without_a_tray_icon_to_come_back_from() {
        assert!(!should_start_hidden(true, Some(true), false));
    }

    /// The flag has to match what the plugin registers, or the login item
    /// writes one string into the plist and the app looks for another — and
    /// every launch would silently show the window.
    #[test]
    fn the_login_flag_is_the_one_the_plugin_registers() {
        assert_eq!(LOGIN_ITEM_FLAG, "--from-autostart");
    }
}
