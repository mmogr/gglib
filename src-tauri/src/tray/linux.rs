//! `StatusNotifierItem` tray backend, used on Linux instead of Tauri's.
//!
//! Tauri's tray goes through `libappindicator`, which registers its item with
//! `ItemIsMenu` and delivers no click events at all: `on_tray_icon_event` never
//! fires and `TrayIcon::rect` is always `None`. That is why the panel could
//! only be opened from a menu entry, and why it had no icon position to anchor
//! to.
//!
//! Talking SNI directly fixes both. `Activate` carries the screen coordinates
//! of the click — the spec calls them "a hint to the item where to show
//! eventual windows" — which is exactly what [`super::layer_shell`] needs to
//! put the panel beside the icon.
//!
//! The menu is [`super::items::ITEMS`] and the routing is
//! [`super::handlers::dispatch`], both shared with the `muda` backend, so the
//! two cannot disagree about what the tray offers or what an entry does.

use ksni::menu::StandardItem;
use ksni::{Handle as ServiceHandle, Icon, MenuItem, ToolTip, Tray, TrayMethods};
use tauri::AppHandle;
use tracing::{error, info};

use crate::tray::icon::{self, TrayVisual};
use crate::tray::items::{ITEMS, Item, is_enabled};
use crate::tray::placement::Anchor;
use crate::tray::{handlers, ids, window};

/// The tray's own state. `ksni` regenerates the menu from this whenever
/// [`ServiceHandle::update`] runs, so `sync` only has to assign fields.
struct GglibTray {
    app: AppHandle,
    /// Endpoint status, shared with the tooltip and the menu's header.
    status: String,
    active: bool,
    proxy_running: bool,
}

impl Tray for GglibTray {
    fn id(&self) -> String {
        icon::TRAY_ID.to_owned()
    }

    fn title(&self) -> String {
        self.status.clone()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        pixmap(self.active)
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.status.clone(),
            ..ToolTip::default()
        }
    }

    /// Left click: open the panel where the icon is.
    ///
    /// `MENU_ON_ACTIVATE` is left at its default of `false` so this runs
    /// instead of the menu opening — the click gesture macOS has always had.
    fn activate(&mut self, x: i32, y: i32) {
        // ksni runs on a tokio task, and everything below eventually reaches
        // GTK, which is main-thread-only. Hopping threads here rather than in
        // `layer_shell` keeps that requirement at the boundary where the
        // foreign event actually arrives.
        let app = self.app.clone();
        let target = self.app.clone();

        if let Err(e) = app.run_on_main_thread(move || {
            if let Err(e) = window::toggle_panel(&target, Anchor::Point { x, y }) {
                error!(error = %e, "Failed to open tray panel");
            }
        }) {
            error!(error = %e, "Could not reach the main thread to open the panel");
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        ITEMS
            .iter()
            .map(|item| match *item {
                // Disabled: a label, not a command. It is the only place the
                // endpoint is visible on Linux, where tooltips are unreliable.
                Item::Status => StandardItem {
                    label: self.status.clone(),
                    enabled: is_enabled(ids::STATUS, self.proxy_running),
                    ..StandardItem::default()
                }
                .into(),
                Item::Action { id, label } => StandardItem {
                    label: label.to_owned(),
                    enabled: is_enabled(id, self.proxy_running),
                    // Hands straight off; `dispatch` spawns anything slow, as
                    // ksni requires of a menu callback.
                    activate: Box::new(move |tray: &mut Self| handlers::dispatch(&tray.app, id)),
                    ..StandardItem::default()
                }
                .into(),
                Item::Separator => MenuItem::Separator,
            })
            .collect()
    }
}

/// The tray icon as SNI wants it.
///
/// Reuses the already-decoded PNGs rather than adding an image dependency:
/// Tauri hands back RGBA, SNI wants ARGB32 in network byte order, so the only
/// work is moving the alpha byte to the front of each pixel.
#[allow(
    clippy::cast_possible_wrap,
    reason = "icon dimensions are far below i32::MAX"
)]
fn pixmap(active: bool) -> Vec<Icon> {
    let Ok(image) = icon::for_state(active) else {
        error!("Failed to decode tray icon");
        return Vec::new();
    };

    let data = image
        .rgba()
        .chunks_exact(4)
        .flat_map(|px| [px[3], px[0], px[1], px[2]])
        .collect();

    vec![Icon {
        width: image.width() as i32,
        height: image.height() as i32,
        data,
    }]
}

/// A registered tray, kept alive for as long as the item should exist.
pub struct Handle(ServiceHandle<GglibTray>);

/// Register the tray with the desktop's `StatusNotifierWatcher`.
///
/// Fails when nothing is watching — a bare window manager with no tray host —
/// which the caller treats exactly as it treats a failed Tauri tray: the app
/// runs, and `autostart::should_start_hidden` refuses to hide the window.
pub fn build(app: &AppHandle) -> Result<Handle, String> {
    let visual = icon::derive(false, None);
    let tray = GglibTray {
        app: app.clone(),
        status: visual.status,
        active: visual.active,
        proxy_running: false,
    };

    // Blocking here matches the rest of `setup_app`, which is not itself inside
    // a task; the registration is a couple of D-Bus round trips.
    let handle = tauri::async_runtime::block_on(tray.spawn())
        .map_err(|e| format!("Failed to register the tray over D-Bus: {e}"))?;

    info!("System tray registered over StatusNotifierItem");
    Ok(Handle(handle))
}

/// Apply proxy state to the icon, tooltip and menu.
///
/// One `update` for all three: `ksni` rebuilds the menu from the tray's state
/// after the closure returns, so there is nothing to keep in step by hand.
pub async fn sync(handle: &Handle, visual: &TrayVisual, proxy_running: bool) -> Result<(), String> {
    handle
        .0
        .update(|tray| {
            tray.status.clone_from(&visual.status);
            tray.active = visual.active;
            tray.proxy_running = proxy_running;
        })
        .await;

    Ok(())
}
