//! macOS Dock icon visibility.
//!
//! With `close_to_tray` on, closing the window leaves gglib running as a proxy
//! host. Keeping a Dock icon for it makes it read as an open app you happen to
//! have hidden, rather than the background service it now is — so the icon goes
//! away with the window and comes back with it.
//!
//! macOS ties this to the application's *activation policy*, and an `Accessory`
//! app is absent from the Cmd+Tab switcher as well as the Dock. There is no way
//! to have one without the other, which is why every caller of [`show`] matters:
//! while hidden, the tray icon is the only way back.
//!
//! Both operations are best-effort. A Dock icon in the wrong state is cosmetic,
//! and no caller has a better answer than carrying on and saying so in the log.
//!
//! Everything here is a no-op off macOS, so call sites need no `#[cfg]` of their
//! own. Windows and Linux have no equivalent notion: the tray icon is separate
//! from the taskbar button, and hiding the window already removes the latter.

#[cfg(target_os = "macos")]
use tauri::ActivationPolicy;
use tauri::AppHandle;

/// Take gglib out of the Dock and the Cmd+Tab switcher.
///
/// Call only once the window is hidden. The policy change is what makes the app
/// menu bar disappear too, which would be visible as a flicker if a window were
/// still on screen.
pub(crate) fn hide(app: &AppHandle) {
    set_policy(app, Policy::Accessory);
}

/// Put gglib back in the Dock and the Cmd+Tab switcher.
///
/// Call *before* showing and focusing the window: an `Accessory` app cannot
/// become frontmost, so a window shown first would appear behind whatever the
/// user was using.
pub(crate) fn show(app: &AppHandle) {
    set_policy(app, Policy::Regular);
}

/// Which way round a [`set_policy`] call goes, kept platform-independent so the
/// logging below reads the same everywhere.
#[derive(Clone, Copy)]
enum Policy {
    Regular,
    Accessory,
}

impl Policy {
    /// Only the macOS path logs, so off macOS this would be dead code.
    #[cfg(any(target_os = "macos", test))]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Regular => "regular",
            Self::Accessory => "accessory",
        }
    }
}

#[cfg(target_os = "macos")]
fn set_policy(app: &AppHandle, policy: Policy) {
    let requested = match policy {
        Policy::Regular => ActivationPolicy::Regular,
        Policy::Accessory => ActivationPolicy::Accessory,
    };

    match app.set_activation_policy(requested) {
        Ok(()) => tracing::debug!(policy = policy.as_str(), "Activation policy set"),
        // Worth a warning rather than a debug: a failed `show` leaves the user
        // with no Dock icon and no Cmd+Tab entry, so this is the line that
        // explains an app they can only reach from the menu bar.
        Err(e) => tracing::warn!(
            error = %e,
            policy = policy.as_str(),
            "Could not set activation policy; dock icon may be wrong"
        ),
    }
}

#[cfg(not(target_os = "macos"))]
fn set_policy(_app: &AppHandle, _policy: Policy) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The labels exist to make the log line readable; asserting them keeps a
    /// rename from quietly turning the log into nonsense.
    #[test]
    fn policies_are_named_for_the_log() {
        assert_eq!(Policy::Regular.as_str(), "regular");
        assert_eq!(Policy::Accessory.as_str(), "accessory");
    }
}
