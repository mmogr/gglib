//! Where the tray panel appears, and the one place that knows platforms differ.
//!
//! Callers describe *what they know about the tray icon* — a rectangle from a
//! click, a point, or nothing — and this module turns that into a position by
//! whatever means the platform offers. Keeping the `#[cfg]` here means
//! [`super::window`], [`super::build`] and [`super::handlers`] need none of
//! their own.

#[cfg(not(target_os = "linux"))]
use tauri::Rect;
use tauri::WebviewWindow;
#[cfg(any(not(target_os = "linux"), test))]
use tauri::{PhysicalPosition, PhysicalSize};

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
#[cfg_attr(
    target_os = "linux",
    expect(
        clippy::unnecessary_wraps,
        reason = "on Linux every arm is `Ok`; elsewhere the `Rect` arm can fail, and callers \
                  handle one signature"
    )
)]
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
/// The arithmetic is [`panel_origin`]'s.
#[cfg(not(target_os = "linux"))]
fn position_near(panel: &WebviewWindow, anchor: Rect) -> tauri::Result<()> {
    let scale = panel.scale_factor().unwrap_or(1.0);
    let anchor_position: PhysicalPosition<f64> = anchor.position.to_physical(scale);
    let anchor_size = anchor.size.to_physical::<f64>(scale);
    let panel_size = panel.outer_size()?;

    let monitor = match panel.current_monitor() {
        Ok(Some(monitor)) => Some((*monitor.position(), *monitor.size())),
        _ => None,
    };

    panel.set_position(panel_origin(
        anchor_position,
        anchor_size,
        panel_size,
        monitor,
    ))
}

/// Where the panel's top-left corner goes, in physical pixels, for a tray icon
/// at `icon_position` of `icon_size`.
///
/// The panel is centred under the icon. With a `monitor` (its origin and size)
/// it is also kept between that monitor's left and right edges, and put above
/// the icon when below would cross the monitor's bottom edge, as it would on a
/// bottom-of-screen tray. Without one it is placed under the icon unclamped.
///
/// Pure. Compiled wherever `position_near`, its one caller, is, and on every
/// system under test, so its tests run on all three.
#[cfg(any(not(target_os = "linux"), test))]
fn panel_origin(
    icon_position: PhysicalPosition<f64>,
    icon_size: PhysicalSize<f64>,
    panel_size: PhysicalSize<u32>,
    monitor: Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>,
) -> PhysicalPosition<f64> {
    let mut x = icon_position.x + (icon_size.width / 2.0) - (f64::from(panel_size.width) / 2.0);
    let mut y = icon_position.y + icon_size.height;

    if let Some((origin, area)) = monitor {
        // Floored at the left edge, because a panel wider than the monitor
        // would otherwise give an upper bound below the lower one, which
        // `clamp` panics on.
        let max_x = f64::from(origin.x) + f64::from(area.width) - f64::from(panel_size.width);
        x = x.clamp(f64::from(origin.x), max_x.max(f64::from(origin.x)));

        let below_bottom =
            y + f64::from(panel_size.height) > f64::from(origin.y) + f64::from(area.height);
        if below_bottom {
            y = icon_position.y - f64::from(panel_size.height);
        }
    }

    PhysicalPosition::new(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANEL: PhysicalSize<u32> = PhysicalSize::new(360, 480);
    const ICON: PhysicalSize<f64> = PhysicalSize::new(24.0, 40.0);

    /// A 1920x1080 monitor whose top-left corner is at (`x`, `y`).
    fn monitor_at(x: i32, y: i32) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
        (PhysicalPosition::new(x, y), PhysicalSize::new(1920, 1080))
    }

    fn corner_for(
        icon_x: f64,
        icon_y: f64,
        monitor: Option<(PhysicalPosition<i32>, PhysicalSize<u32>)>,
    ) -> PhysicalPosition<f64> {
        panel_origin(PhysicalPosition::new(icon_x, icon_y), ICON, PANEL, monitor)
    }

    /// A menu bar icon mid-screen: the panel hangs from the icon's bottom edge,
    /// its centre on the icon's centre (900 + 12 - 180).
    #[test]
    fn the_panel_is_centred_under_the_icon() {
        let at = corner_for(900.0, 0.0, Some(monitor_at(0, 0)));

        assert_eq!(at, PhysicalPosition::new(732.0, 40.0));
    }

    /// An icon near the right-hand edge, where menu bar items usually are,
    /// would push half the panel off; it stops at the edge instead.
    #[test]
    fn an_icon_at_the_right_edge_keeps_the_panel_on_the_monitor() {
        let at = corner_for(1890.0, 0.0, Some(monitor_at(0, 0)));

        assert_eq!(at, PhysicalPosition::new(1920.0 - 360.0, 40.0));
    }

    #[test]
    fn an_icon_at_the_left_edge_keeps_the_panel_on_the_monitor() {
        let at = corner_for(10.0, 0.0, Some(monitor_at(0, 0)));

        assert_eq!(at, PhysicalPosition::new(0.0, 40.0));
    }

    /// A bottom-of-screen tray, Windows' default: below the icon is off the
    /// monitor, so the panel's bottom edge meets the icon's top edge instead.
    #[test]
    fn an_icon_at_the_bottom_puts_the_panel_above_it() {
        let at = corner_for(1700.0, 1040.0, Some(monitor_at(0, 0)));

        assert_eq!(at, PhysicalPosition::new(1532.0, 1040.0 - 480.0));
    }

    /// A panel whose bottom edge lands exactly on the monitor's stays below.
    #[test]
    fn a_panel_that_just_fits_below_stays_below() {
        // 560 + 40 + 480 = 1080.
        let at = corner_for(900.0, 560.0, Some(monitor_at(0, 0)));

        assert_eq!(at, PhysicalPosition::new(732.0, 600.0));
    }

    /// A second monitor to the right of and below the first: both edges are
    /// measured from its own origin, not from (0, 0).
    #[test]
    fn a_monitor_away_from_the_origin_clamps_to_its_own_edges() {
        let monitor = Some(monitor_at(1920, 200));

        // Centred would start at 1750, left of this monitor.
        let left = corner_for(1918.0, 200.0, monitor);
        assert_eq!(left, PhysicalPosition::new(1920.0, 240.0));

        let right = corner_for(3830.0, 200.0, monitor);
        assert_eq!(right, PhysicalPosition::new(1920.0 + 1920.0 - 360.0, 240.0));
    }

    /// 700 + 40 + 480 = 1220 crosses 1080 but not this monitor's bottom at
    /// 200 + 1080, so the panel stays below; 1240 + 40 + 480 crosses both.
    #[test]
    fn a_monitor_away_from_the_origin_has_its_own_bottom() {
        let at = corner_for(2900.0, 700.0, Some(monitor_at(1920, 200)));

        assert_eq!(at, PhysicalPosition::new(2732.0, 740.0));

        let at = corner_for(2900.0, 1240.0, Some(monitor_at(1920, 200)));

        assert_eq!(at, PhysicalPosition::new(2732.0, 1240.0 - 480.0));
    }

    /// A monitor to the left of and above the primary has a negative origin.
    #[test]
    fn a_monitor_with_a_negative_origin_clamps_to_its_own_edges() {
        let monitor = Some(monitor_at(-1920, -1080));

        let at = corner_for(-1910.0, -1080.0, monitor);
        assert_eq!(at, PhysicalPosition::new(-1920.0, -1040.0));

        let at = corner_for(-20.0, -40.0, monitor);
        assert_eq!(at, PhysicalPosition::new(-360.0, -40.0 - 480.0));
    }

    /// A panel wider than the monitor would invert the clamp's bounds, which
    /// panics; it is pinned to the left edge instead.
    #[test]
    fn a_panel_wider_than_the_monitor_does_not_panic() {
        let small = Some((PhysicalPosition::new(100, 0), PhysicalSize::new(320, 1080)));

        let at = corner_for(300.0, 0.0, small);

        assert_eq!(at, PhysicalPosition::new(100.0, 40.0));
    }

    /// With no monitor known nothing is clamped or flipped.
    #[test]
    fn without_a_monitor_the_panel_is_only_centred() {
        let at = corner_for(1890.0, 5000.0, None);

        assert_eq!(at, PhysicalPosition::new(1722.0, 5040.0));
    }
}
