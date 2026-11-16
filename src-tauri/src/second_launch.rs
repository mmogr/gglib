//! A second launch of the desktop app.
//!
//! `tauri-plugin-single-instance`, the first plugin `main` registers, looks
//! for a running copy with the same bundle identifier (a named mutex in the
//! login session on Windows, a name on the session bus on Linux, a socket in
//! `/tmp` on macOS). If one answers, this launch hands it its arguments and
//! working directory and exits, before it has built a window or reached the
//! daemon. The running copy calls [`on_second_launch`]: its
//! main window comes forward, from the tray too, and the page is told what
//! the launch carried (`second-launch`), so a deep link can be acted on there
//! later. Nothing acts on the arguments yet.
//!
//! The decision is [`second_launch`], a plain function, so it is tested
//! without a running app; the callback only carries it out.

use serde::Serialize;
use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Emitter, Wry};
use tracing::{error, info};

use crate::autostart::LOGIN_ITEM_FLAG;
use crate::tray;

/// The event the main window hears a second launch by.
/// `src/services/platform/secondLaunch.ts` listens for the same name.
pub(crate) const EVENT: &str = "second-launch";

/// Arguments gglib passes itself, which mean nothing to the page. The login
/// item's flag hides the window of a first launch; on a second one the
/// window is shown all the same.
const OWN_FLAGS: &[&str] = &[LOGIN_ITEM_FLAG];

/// What a second launch carried, as the page receives it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SecondLaunchPayload {
    /// The arguments, in order, without the program and gglib's own flags.
    pub(crate) args: Vec<String>,
    /// The folder the second launch was started in.
    pub(crate) cwd: String,
}

/// What to do about a second launch.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SecondLaunch {
    /// Whether the main window comes forward. Always, by decision D5 of
    /// #1959: someone who launches gglib again wants to see it, however
    /// the first launch left it.
    pub(crate) show_main_window: bool,
    /// What the main window is sent.
    pub(crate) payload: SecondLaunchPayload,
}

/// Decide what a second launch does, from its whole command line (the
/// program first, as the plugin passes it) and its working directory.
pub(crate) fn second_launch(args: Vec<String>, cwd: String) -> SecondLaunch {
    let args = args
        .into_iter()
        .skip(1)
        .filter(|arg| !OWN_FLAGS.contains(&arg.as_str()))
        .collect();
    SecondLaunch {
        show_main_window: true,
        payload: SecondLaunchPayload { args, cwd },
    }
}

/// The single-instance plugin, calling [`on_second_launch`]. `main`
/// registers it before every other plugin, as the plugin requires.
pub(crate) fn plugin() -> TauriPlugin<Wry> {
    tauri_plugin_single_instance::init(on_second_launch)
}

/// The single-instance plugin's callback: carry out [`second_launch`].
///
/// Touching no app state, because the plugin may call it before `setup` has
/// managed any. The window work is spawned, off the thread the plugin calls
/// from (on Windows the event thread, while the second process waits), as
/// the tray's Open gglib does; Windows grants the focus to the process, so
/// it still applies there.
fn on_second_launch(app: &AppHandle, args: Vec<String>, cwd: String) {
    let SecondLaunch {
        show_main_window,
        payload,
    } = second_launch(args, cwd);
    // The count only: an argument may one day be a link carrying a code.
    info!(
        arguments = payload.args.len(),
        "gglib was launched again; showing the first window"
    );

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if show_main_window && let Err(e) = tray::window::show_main(&app) {
            error!(error = %e, "Could not show the main window for a second launch");
        }
        if let Err(e) = app.emit_to(tray::window::MAIN_LABEL, EVENT, payload) {
            error!(error = %e, event = EVENT, "Could not tell the main window about a second launch");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{SecondLaunch, SecondLaunchPayload, second_launch};
    use crate::autostart::LOGIN_ITEM_FLAG;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_arguments_are_forwarded_in_order_without_the_program() {
        let launch = second_launch(
            strings(&[
                "/Applications/gglib.app/Contents/MacOS/gglib-gui",
                "gglib://open",
                "--x",
                "b",
            ]),
            "/Users/someone".to_owned(),
        );

        assert_eq!(
            launch,
            SecondLaunch {
                show_main_window: true,
                payload: SecondLaunchPayload {
                    args: strings(&["gglib://open", "--x", "b"]),
                    cwd: "/Users/someone".to_owned(),
                },
            }
        );
    }

    #[test]
    fn the_login_item_flag_does_not_keep_the_window_hidden() {
        let launch = second_launch(
            strings(&["/usr/bin/gglib-gui", "a", LOGIN_ITEM_FLAG, "b"]),
            "/home/someone".to_owned(),
        );

        assert_eq!(
            launch,
            SecondLaunch {
                show_main_window: true,
                payload: SecondLaunchPayload {
                    args: strings(&["a", "b"]),
                    cwd: "/home/someone".to_owned(),
                },
            }
        );
    }

    #[test]
    fn an_empty_argument_list_still_shows_the_window() {
        assert_eq!(
            second_launch(Vec::new(), String::new()),
            SecondLaunch {
                show_main_window: true,
                payload: SecondLaunchPayload {
                    args: Vec::new(),
                    cwd: String::new(),
                },
            }
        );
    }

    #[test]
    fn the_payload_reaches_the_page_as_its_args_and_cwd() {
        let payload = second_launch(strings(&["gglib-gui", "a"]), "/x".to_owned()).payload;

        assert_eq!(
            serde_json::to_value(&payload).unwrap(),
            serde_json::json!({ "args": ["a"], "cwd": "/x" })
        );
    }
}
