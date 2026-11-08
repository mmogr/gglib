//! Deriving the tray's appearance from the daemon's state.
//!
//! Pure functions with no `AppHandle` and no I/O, so the mapping from "what
//! gglib is doing" to "what the icon looks like" is directly testable without
//! a running Tauri application.

use gglib_core::domain::ResidencyState;
use tauri::image::Image;

use crate::daemon::DaemonSnapshot;

/// Identifier the tray registers under.
pub(super) const TRAY_ID: &str = "gglib";

/// How much of the bubble's opacity the outline icon keeps.
const OUTLINE_OPACITY_PERCENT: u16 = 35;

/// The colour the glyph is drawn in where the system does not recolour it:
/// the app icon's blue.
///
/// macOS recolours a template image for its menu bar. Windows and Linux show
/// the icon's own pixels, and the black glyph all but disappears on a dark
/// taskbar or panel, Windows 11's default and GNOME's top bar among them. The
/// blue reads on a light bar and a dark one.
const TINT: [u8; 3] = [0x3d, 0x8b, 0xe0];

/// What the tray icon is saying: the endpoint, and whether anything is
/// loaded behind it.
///
/// Two glyphs ship, drawn by `scripts/icons/build.py` from the app icon's
/// speech bubble: the bubble with the llama cut out of it (`tray-idle.png`)
/// and the same with a dot (`tray-active.png`). The outline is the bubble
/// faded. Three icons, told apart, none animated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrayState {
    /// gglib is not running, or its endpoint is off: clients cannot connect.
    Outline,
    /// The endpoint is on (or pinned) and nothing is loaded.
    Solid,
    /// The endpoint is on (or pinned) and at least one model is loaded.
    SolidDot,
}

/// The bubble: the endpoint on, nothing loaded.
pub(super) fn idle_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../../icons/tray-idle.png"))
}

/// The bubble with a dot: the endpoint on, something loaded.
pub(super) fn active_icon() -> tauri::Result<Image<'static>> {
    Image::from_bytes(include_bytes!("../../icons/tray-active.png"))
}

/// The bubble, faded: no endpoint to connect to.
///
/// Derived from the bubble rather than shipped as a third asset. It is the
/// same glyph, so a separate file could only drift from it, and fading needs
/// no image dependency — the same reasoning as the ARGB byte swap in
/// [`super::linux`]. Both icons carry their glyph entirely in the alpha
/// channel, which is what macOS template mode requires, so scaling alpha is
/// exactly "draw the same thing fainter" on every backend.
pub(super) fn outline_icon() -> tauri::Result<Image<'static>> {
    let idle = idle_icon()?;
    let (width, height) = (idle.width(), idle.height());

    let faded = idle
        .rgba()
        .chunks_exact(4)
        .flat_map(|px| [px[0], px[1], px[2], fade(px[3])])
        .collect();

    Ok(Image::new_owned(faded, width, height))
}

/// One alpha byte, dimmed.
const fn fade(alpha: u8) -> u8 {
    // Cannot overflow: the largest product is 255 * 35, and dividing by 100
    // lands well inside u8.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "255 * OUTLINE_OPACITY_PERCENT / 100 is at most 89"
    )]
    {
        (alpha as u16 * OUTLINE_OPACITY_PERCENT / 100) as u8
    }
}

/// The icon for a state, already decoded, in this platform's colour.
pub(super) fn for_state(state: TrayState) -> tauri::Result<Image<'static>> {
    let glyph = match state {
        TrayState::Outline => outline_icon(),
        TrayState::Solid => idle_icon(),
        TrayState::SolidDot => active_icon(),
    }?;

    Ok(if cfg!(target_os = "macos") {
        glyph
    } else {
        tinted(&glyph)
    })
}

/// The same glyph in [`TINT`].
///
/// The shape is entirely in the alpha channel, so replacing the colour and
/// keeping alpha redraws it exactly, with no image dependency, as
/// [`outline_icon`] fades it.
fn tinted(glyph: &Image<'_>) -> Image<'static> {
    let [r, g, b] = TINT;
    let rgba = glyph
        .rgba()
        .chunks_exact(4)
        .flat_map(|px| [r, g, b, px[3]])
        .collect();

    Image::new_owned(rgba, glyph.width(), glyph.height())
}

/// How the tray should look for a given daemon state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TrayVisual {
    /// Which icon to show.
    pub state: TrayState,
    /// Hover text, and the label of the menu's status line.
    ///
    /// One string for both on purpose: one sentence from one snapshot (TM-5).
    /// Linux shows no hover text, so there the status line is the only place
    /// it is read.
    pub status: String,
}

/// Derive the tray's appearance.
///
/// The icon answers the question a menu bar item is asked: can a client
/// connect, and is anything loaded behind it. A model loaded with the endpoint
/// off draws the outline — no client can reach it — and the status says it is
/// loaded.
#[must_use]
pub(super) fn derive(snap: &DaemonSnapshot) -> TrayVisual {
    TrayVisual {
        state: state_of(snap),
        status: status_of(snap),
    }
}

/// Which icon a snapshot calls for.
fn state_of(snap: &DaemonSnapshot) -> TrayState {
    if !snap.reachable || !snap.proxy_running {
        TrayState::Outline
    } else if snap.resident.is_empty() {
        TrayState::Solid
    } else {
        TrayState::SolidDot
    }
}

/// The one sentence the tooltip and the menu's status line share:
/// "gglib · endpoint on · Qwen3-4B loaded" and its siblings.
///
/// The state words are `gglib_core`'s, the ones every surface says, in lower
/// case mid-sentence.
fn status_of(snap: &DaemonSnapshot) -> String {
    if !snap.reachable {
        return "gglib · not running".to_owned();
    }

    let endpoint = format!("gglib · endpoint {}", snap.endpoint.word().to_lowercase());
    match loaded_phrase(snap) {
        Some(loaded) => format!("{endpoint} · {loaded}"),
        None => endpoint,
    }
}

/// What is loaded, if anything: one model by name, more by count.
fn loaded_phrase(snap: &DaemonSnapshot) -> Option<String> {
    let loaded = ResidencyState::Loaded.word().to_lowercase();
    match snap.resident.as_slice() {
        [] => None,
        [one] if !one.name.is_empty() => Some(format!("{} {loaded}", one.name)),
        [_] => Some(format!("1 model {loaded}")),
        many => Some(format!("{} models {loaded}", many.len())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Build a snapshot the way the watcher does, so these tests exercise the
    /// same construction the app runs.
    fn snapshot(proxy: &serde_json::Value, servers: &serde_json::Value) -> DaemonSnapshot {
        DaemonSnapshot::from_responses(proxy, servers)
    }

    fn on() -> serde_json::Value {
        json!({"running": true, "port": 8080, "state": "on"})
    }

    #[test]
    fn no_daemon_reads_as_not_running() {
        let visual = derive(&DaemonSnapshot::default());

        assert_eq!(visual.state, TrayState::Outline);
        assert_eq!(visual.status, "gglib · not running");
    }

    /// A daemon with its endpoint off is not "not running": it says off.
    #[test]
    fn an_endpoint_off_is_an_outline_that_says_off() {
        let visual = derive(&snapshot(&json!({"running": false}), &json!([])));

        assert_eq!(visual.state, TrayState::Outline);
        assert_eq!(visual.status, "gglib · endpoint off");
    }

    #[test]
    fn an_endpoint_on_with_nothing_loaded_is_solid() {
        let visual = derive(&snapshot(&on(), &json!([])));

        assert_eq!(visual.state, TrayState::Solid);
        assert_eq!(visual.status, "gglib · endpoint on");
    }

    #[test]
    fn an_endpoint_on_with_a_model_loaded_has_the_dot_and_names_it() {
        let visual = derive(&snapshot(
            &on(),
            &json!([{"model_id": 1, "model_name": "Qwen3-4B"}]),
        ));

        assert_eq!(visual.state, TrayState::SolidDot);
        assert_eq!(visual.status, "gglib · endpoint on · Qwen3-4B loaded");
    }

    #[test]
    fn two_loaded_models_are_counted() {
        let visual = derive(&snapshot(
            &on(),
            &json!([{"model_id": 1, "model_name": "Qwen3-4B"}, {"model_id": 2, "model_name": "Gemma-3-4B-It"}]),
        ));

        assert_eq!(visual.status, "gglib · endpoint on · 2 models loaded");
    }

    /// No client can reach a model loaded behind an endpoint that is off, so
    /// the icon is the outline; the status still says what is loaded.
    #[test]
    fn a_model_loaded_with_the_endpoint_off_draws_the_outline() {
        let visual = derive(&snapshot(
            &json!({"running": false}),
            &json!([{"model_id": 1, "model_name": "Qwen3-4B"}]),
        ));

        assert_eq!(visual.state, TrayState::Outline);
        assert_eq!(visual.status, "gglib · endpoint off · Qwen3-4B loaded");
    }

    /// A pinned run says the daemon's own word for it.
    #[test]
    fn a_pinned_endpoint_says_pinned() {
        let visual = derive(&snapshot(
            &json!({"running": true, "port": 8080, "state": "pinned"}),
            &json!([{"model_id": 1, "model_name": "Qwen3-4B"}]),
        ));

        assert_eq!(visual.state, TrayState::SolidDot);
        assert_eq!(visual.status, "gglib · endpoint pinned · Qwen3-4B loaded");
    }

    /// A server entry without a name still counts, by number.
    #[test]
    fn a_loaded_model_without_a_name_is_counted() {
        let visual = derive(&snapshot(&on(), &json!([{"model_id": 1}])));

        assert_eq!(visual.status, "gglib · endpoint on · 1 model loaded");
    }

    /// An unreachable daemon says so and nothing else — no leftover endpoint
    /// or model from before it went away.
    #[test]
    fn an_unreachable_daemon_reports_nothing_left_over() {
        let stale = DaemonSnapshot {
            reachable: false,
            ..snapshot(&on(), &json!([{"model_id": 1, "model_name": "Qwen3-4B"}]))
        };

        assert_eq!(derive(&stale).state, TrayState::Outline);
        assert_eq!(derive(&stale).status, "gglib · not running");
    }

    /// The outline icon is derived at runtime, so it has to actually come out
    /// the same shape as the bubble it is derived from.
    #[test]
    fn the_outline_icon_is_the_bubble_dimmed() {
        let bubble = idle_icon().expect("bubble icon decodes");
        let outline = outline_icon().expect("outline icon derives");

        assert_eq!(outline.width(), bubble.width());
        assert_eq!(outline.height(), bubble.height());

        let brightest_bubble = bubble.rgba().chunks_exact(4).map(|px| px[3]).max();
        let brightest_outline = outline.rgba().chunks_exact(4).map(|px| px[3]).max();
        assert!(
            brightest_outline < brightest_bubble,
            "the outline icon must be fainter than the bubble"
        );
        assert!(
            brightest_outline > Some(0),
            "but still visible, or there is no icon to click"
        );
    }

    /// Tinting changes the colour and nothing else: every pixel keeps its
    /// alpha, so the shape is the glyph's, and every visible one is the blue.
    #[test]
    fn the_tint_keeps_the_shape_and_takes_the_blue() {
        let glyph = active_icon().expect("icon decodes");
        let tinted = tinted(&glyph);

        assert_eq!(
            (tinted.width(), tinted.height()),
            (glyph.width(), glyph.height())
        );
        for (before, after) in glyph
            .rgba()
            .chunks_exact(4)
            .zip(tinted.rgba().chunks_exact(4))
        {
            assert_eq!(after[3], before[3], "alpha is the shape and must not move");
            assert_eq!(after[..3], TINT, "every pixel takes the tint");
        }
        assert!(
            glyph.rgba().chunks_exact(4).any(|px| px[..3] != TINT),
            "the shipped glyph is not already the tint, or this test proves nothing"
        );
    }

    /// macOS gets the black template it recolours itself; everywhere else the
    /// glyph comes tinted, or it vanishes on a dark bar.
    #[test]
    fn the_glyph_is_tinted_only_where_the_system_does_not_recolour_it() {
        let shown = for_state(TrayState::Solid).expect("icon");
        let visible = shown
            .rgba()
            .chunks_exact(4)
            .find(|px| px[3] == u8::MAX)
            .expect("the glyph has opaque pixels");

        let expected = if cfg!(target_os = "macos") {
            [0, 0, 0]
        } else {
            TINT
        };
        assert_eq!(visible[..3], expected);
    }

    /// Three states, three different images.
    #[test]
    fn each_state_draws_its_own_icon() {
        let rgba = |state| for_state(state).expect("icon").rgba().to_vec();

        assert_ne!(rgba(TrayState::Outline), rgba(TrayState::Solid));
        assert_ne!(rgba(TrayState::Solid), rgba(TrayState::SolidDot));
        assert_ne!(rgba(TrayState::Outline), rgba(TrayState::SolidDot));
    }
}
