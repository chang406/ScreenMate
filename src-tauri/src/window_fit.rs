//! Keeps the transparent overlay window usable on any monitor size / DPI scale.
//!
//! The overlay is a fixed 640x920 (logical) transparent window whose character stands at the
//! *bottom* of the window. On a small or high-scale display (e.g. a 1366x768 laptop at 125%,
//! whose usable height is ~570 logical px) the window is taller than the screen, so its bottom
//! - and with it the character - ends up off-screen where it can be neither seen nor dragged.
//!
//! This module:
//! - shrinks the window to fit the monitor's work area (taskbar excluded); the frontend scales the
//!   character from the window size, so the character shrinks with it;
//! - starts the app with the character at the centre of the primary monitor;
//! - never moves the character after the *user* drags it (peeking from the screen edge or
//!   flicking it away to hide is intended use) - only the window size is re-fitted when it lands
//!   on a monitor of a different size / scale;
//! - offers "bring to screen centre" (overlay context menu and tray) as the escape hatch when the
//!   character has been moved out of reach.
//!
//! The placement maths is pure (`plan_placement`) and unit-tested; `refit_main` is the Tauri glue.

use std::sync::Mutex;
use tauri::{PhysicalPosition, PhysicalSize, Runtime, WebviewWindow};

/// Design size of the overlay in logical pixels (see `tauri.conf.json`).
pub const DESIGN_LOGICAL_SIZE: (f64, f64) = (640.0, 920.0);

/// Vertical span of the visible character inside the window, as fractions of the window height
/// (0 = window top, 1 = window bottom). Reported by the overlay (`report_character_zone`) because
/// it depends on the character model and its zoom level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterZone {
    pub top: f64,
    pub bottom: f64,
}

impl Default for CharacterZone {
    /// Conservative guess used until the overlay reports the real span: the character occupies
    /// roughly the lower half of the window at 100% zoom.
    fn default() -> Self {
        Self {
            top: 0.45,
            bottom: 0.98,
        }
    }
}

static CHARACTER_ZONE: Mutex<Option<CharacterZone>> = Mutex::new(None);

fn current_zone() -> CharacterZone {
    CHARACTER_ZONE
        .lock()
        .ok()
        .and_then(|z| *z)
        .unwrap_or_default()
}

/// Called by the overlay whenever the character's on-screen span changes (character switch,
/// zoom, window resize). Used by "bring to centre" to centre the *character* rather than the
/// window, and to re-centre once at startup when the real size becomes known.
#[tauri::command]
pub fn report_character_zone(window: tauri::Window, top: f64, bottom: f64) {
    if !(top.is_finite() && bottom.is_finite()) {
        return;
    }
    let top = top.clamp(0.0, 1.0);
    let bottom = bottom.clamp(0.0, 1.0);
    if bottom - top < 0.02 {
        return;
    }
    let zone = CharacterZone { top, bottom };
    let changed = CHARACTER_ZONE
        .lock()
        .map(|mut z| {
            let changed = z.is_none_or(|old| {
                (old.top - top).abs() > 0.01 || (old.bottom - bottom).abs() > 0.01
            });
            *z = Some(zone);
            changed
        })
        .unwrap_or(false);
    if changed && window.label() == "main" {
        use tauri::Manager;
        if let Some(win) = window.app_handle().get_webview_window("main") {
            crate::desktop::on_character_measured(&win);
        }
    }
}

/// How strictly the character must be kept on-screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitMode {
    /// The whole character must be visible. Used for "bring to centre".
    KeepVisible,
    /// Only fit the window size to the monitor; never move the character. Used after the user
    /// drags the character (or a DPI change while dragging).
    SizeOnly,
}

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    fn right(&self) -> i64 {
        self.x as i64 + self.w as i64
    }
    fn bottom(&self) -> i64 {
        self.y as i64 + self.h as i64
    }
    fn contains(&self, px: i64, py: i64) -> bool {
        px >= self.x as i64 && px < self.right() && py >= self.y as i64 && py < self.bottom()
    }
}

/// A monitor as seen by the placement logic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    /// Full monitor bounds.
    pub bounds: Rect,
    /// Usable area (taskbar / dock excluded).
    pub work: Rect,
    pub scale: f64,
    pub primary: bool,
}

/// Where the window should be and how big it should be (physical px).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// Window size for a monitor: the design size, but never larger than the work area.
pub fn fitted_size(screen: &Screen) -> (u32, u32) {
    let want_w = (DESIGN_LOGICAL_SIZE.0 * screen.scale).round() as u32;
    let want_h = (DESIGN_LOGICAL_SIZE.1 * screen.scale).round() as u32;
    (want_w.min(screen.work.w), want_h.min(screen.work.h))
}

/// The point the character stands on: bottom-centre of the window (slightly above the edge).
fn character_anchor(x: i32, y: i32, w: u32, h: u32) -> (i64, i64) {
    (x as i64 + w as i64 / 2, y as i64 + (h as i64 * 9) / 10)
}

/// Picks the monitor the window belongs to: the one under its character anchor, otherwise the
/// one under the window centre, otherwise the primary (or first) monitor.
pub fn pick_screen(screens: &[Screen], x: i32, y: i32, w: u32, h: u32) -> Option<Screen> {
    let (ax, ay) = character_anchor(x, y, w, h);
    let (cx, cy) = (x as i64 + w as i64 / 2, y as i64 + h as i64 / 2);
    screens
        .iter()
        .find(|s| s.bounds.contains(ax, ay))
        .or_else(|| screens.iter().find(|s| s.bounds.contains(cx, cy)))
        .or_else(|| screens.iter().find(|s| s.primary))
        .or_else(|| screens.first())
        .copied()
}

/// "Bring to centre": the character (not the window) centred in the work area, fully visible.
pub fn center_placement(screen: &Screen, zone: CharacterZone) -> Placement {
    let (w, h) = fitted_size(screen);
    let work = screen.work;
    let x = work.x as i64 + (work.w as i64 - w as i64) / 2;
    let char_mid = ((zone.top + zone.bottom) / 2.0 * h as f64).round() as i64;
    let y = work.y as i64 + work.h as i64 / 2 - char_mid;
    // Run through the strict rule so a huge zoom still keeps head and feet on-screen.
    plan_placement(screen, x as i32, y as i32, w, h, zone, FitMode::KeepVisible)
}

/// Computes a placement that fits `screen` while moving the window as little as possible.
///
/// - Size: shrunk to the work area if needed. When shrinking, the bottom edge stays put
///   (the character stands at the bottom) and the window stays horizontally centred.
/// - Vertical: the *character* (not the whole window) is kept inside the work area: its feet never
///   go below the screen / behind the taskbar and its head never above the screen top. The
///   transparent space above the character may hang above the screen, so the character can be
///   placed anywhere vertically, including near the top of the screen.
/// - Horizontal: up to a quarter of the window may hang off either side. The left/right parts
///   of the window are transparent padding, so this still lets the character sit close to the
///   screen edge, while its body stays on-screen.
pub fn plan_placement(
    screen: &Screen,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    zone: CharacterZone,
    mode: FitMode,
) -> Placement {
    let (fw, fh) = fitted_size(screen);
    // Keep bottom-centre fixed while resizing.
    let mut nx = x as i64 + (w as i64 - fw as i64) / 2;
    let mut ny = y as i64 + h as i64 - fh as i64;

    if mode == FitMode::SizeOnly {
        return Placement {
            x: nx as i32,
            y: ny as i32,
            w: fw,
            h: fh,
        };
    }

    let work = screen.work;
    let slack = fw as i64 / 4;
    let min_x = work.x as i64 - slack;
    let max_x = work.right() - fw as i64 + slack;
    nx = nx.clamp(min_x, max_x.max(min_x));

    let char_top = (zone.top * fh as f64).round() as i64;
    let char_bottom = (zone.bottom * fh as f64).round() as i64;
    let (min_y, max_y) = match mode {
        // Head not above the screen top, feet not below the work-area bottom.
        FitMode::KeepVisible => (work.y as i64 - char_top, work.bottom() - char_bottom),
        FitMode::SizeOnly => unreachable!("handled above"),
    };
    // If the character is taller than the screen (should not happen once fitted), keep the feet.
    ny = if max_y < min_y {
        max_y
    } else {
        ny.clamp(min_y, max_y)
    };

    Placement {
        x: nx as i32,
        y: ny as i32,
        w: fw,
        h: fh,
    }
}

// ---------------------------------------------------------------------------
// Tauri glue
// ---------------------------------------------------------------------------

fn screens_of<R: Runtime>(win: &WebviewWindow<R>) -> Vec<Screen> {
    let primary = win.primary_monitor().ok().flatten().map(|m| *m.position());
    win.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            let wa = m.work_area();
            Screen {
                bounds: Rect {
                    x: p.x,
                    y: p.y,
                    w: s.width,
                    h: s.height,
                },
                work: Rect {
                    x: wa.position.x,
                    y: wa.position.y,
                    w: wa.size.width,
                    h: wa.size.height,
                },
                scale: m.scale_factor(),
                primary: primary == Some(*p),
            }
        })
        .collect()
}

fn apply<R: Runtime>(win: &WebviewWindow<R>, current: Placement, target: Placement) {
    if (target.w, target.h) != (current.w, current.h) {
        let _ = win.set_size(PhysicalSize::new(target.w, target.h));
    }
    if (target.x, target.y) != (current.x, current.y) {
        let _ = win.set_position(PhysicalPosition::new(target.x, target.y));
    }
    if target != current {
        println!(
            "[window_fit] {}x{} @ ({}, {}) -> {}x{} @ ({}, {})",
            current.w, current.h, current.x, current.y, target.w, target.h, target.x, target.y
        );
    }
}

fn current_placement<R: Runtime>(win: &WebviewWindow<R>) -> Option<Placement> {
    let pos = win.outer_position().ok()?;
    let size = win.outer_size().ok()?;
    Some(Placement {
        x: pos.x,
        y: pos.y,
        w: size.width,
        h: size.height,
    })
}

/// Fits the main window to the monitor it is on and pulls it back on-screen if needed.
pub fn refit_main<R: Runtime>(win: &WebviewWindow<R>, mode: FitMode) {
    let Some(cur) = current_placement(win) else {
        return;
    };
    let screens = screens_of(win);
    let Some(screen) = pick_screen(&screens, cur.x, cur.y, cur.w, cur.h) else {
        return;
    };
    apply(
        win,
        cur,
        plan_placement(&screen, cur.x, cur.y, cur.w, cur.h, current_zone(), mode),
    );
}

fn primary_screen(screens: &[Screen]) -> Option<Screen> {
    screens
        .iter()
        .find(|s| s.primary)
        .or_else(|| screens.first())
        .copied()
}

/// Moves the character to the centre of a monitor.
/// `on_primary`: the tray uses the primary monitor (the character may be on a monitor that is
/// gone or out of reach); the overlay's own menu uses the monitor the character is on.
pub fn recenter<R: Runtime>(win: &WebviewWindow<R>, on_primary: bool) {
    let Some(cur) = current_placement(win) else {
        return;
    };
    let screens = screens_of(win);
    let screen = if on_primary {
        primary_screen(&screens)
    } else {
        pick_screen(&screens, cur.x, cur.y, cur.w, cur.h)
    };
    if let Some(screen) = screen {
        apply(win, cur, center_placement(&screen, current_zone()));
    }
}

/// Overlay context menu: "bring to screen centre".
#[tauri::command]
pub fn recenter_main(app: tauri::AppHandle) {
    use tauri::Manager;
    if let Some(win) = app.get_webview_window("main") {
        recenter(&win, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Character occupying the lower half of the window (typical at 100% zoom).
    const Z: CharacterZone = CharacterZone {
        top: 0.5,
        bottom: 0.98,
    };

    /// Screen y of the character's head / feet for a placement.
    fn head_y(p: &Placement, z: CharacterZone) -> i64 {
        p.y as i64 + (z.top * p.h as f64).round() as i64
    }
    fn feet_y(p: &Placement, z: CharacterZone) -> i64 {
        p.y as i64 + (z.bottom * p.h as f64).round() as i64
    }

    fn screen(x: i32, y: i32, w: u32, h: u32, taskbar: u32, scale: f64, primary: bool) -> Screen {
        Screen {
            bounds: Rect { x, y, w, h },
            work: Rect {
                x,
                y,
                w,
                h: h - taskbar,
            },
            scale,
            primary,
        }
    }

    /// 1920x1080 desktop monitor at 100%, 48px taskbar.
    fn desktop() -> Screen {
        screen(0, 0, 1920, 1080, 48, 1.0, true)
    }

    /// 1366x768 laptop at 125%: usable height 768-60 = 708 physical px, but the window wants
    /// 920*1.25 = 1150 physical px.
    fn laptop_125() -> Screen {
        screen(0, 0, 1366, 768, 60, 1.25, true)
    }

    /// 2560x1600 laptop at 150%: wants 960x1380, usable height 1600-72 = 1528 -> fits.
    fn laptop_150_hidpi() -> Screen {
        screen(0, 0, 2560, 1600, 72, 1.5, true)
    }

    #[test]
    fn design_size_kept_when_it_fits() {
        assert_eq!(fitted_size(&desktop()), (640, 920));
        assert_eq!(fitted_size(&laptop_150_hidpi()), (960, 1380));
    }

    #[test]
    fn window_shrinks_to_small_laptop_work_area() {
        // 640*1.25 = 800 wide still fits; height is limited to the work area.
        assert_eq!(fitted_size(&laptop_125()), (800, 708));
    }

    #[test]
    fn oversized_window_on_laptop_brings_character_on_screen() {
        // The reported bug: window at the OS default spot, its bottom (and the character) far
        // below the 768px screen.
        let s = laptop_125();
        let p = plan_placement(&s, 100, 50, 800, 1150, Z, FitMode::KeepVisible);
        assert_eq!((p.w, p.h), (800, 708));
        assert!(head_y(&p, Z) >= s.work.y as i64, "head visible: {p:?}");
        assert!(feet_y(&p, Z) <= s.work.bottom(), "feet visible: {p:?}");
    }

    #[test]
    fn character_can_be_moved_up_to_the_top_of_the_screen() {
        // Reported after the first version of this PR: the character could not be dragged higher
        // because the *whole window* (whose top half is empty) had to stay on-screen.
        // 2560x1440 @100%: dragging the window so the character's head touches the screen top
        // puts the window's transparent top half above the screen - that must be allowed.
        let s = screen(0, 0, 2560, 1440, 48, 1.0, true);
        let wanted_y = -(0.5 * 920.0) as i32; // head exactly at y = 0
        let p = plan_placement(&s, 1000, wanted_y, 640, 920, Z, FitMode::KeepVisible);
        assert_eq!(p.y, wanted_y, "must not be pushed back down");
        assert_eq!(head_y(&p, Z), 0);
        // ...but the head itself may not go above the screen.
        let p = plan_placement(&s, 1000, wanted_y - 200, 640, 920, Z, FitMode::KeepVisible);
        assert_eq!(head_y(&p, Z), 0);
    }

    #[test]
    fn bigger_zoom_limits_how_high_the_window_can_go() {
        // At 200% zoom the character fills almost the whole window, so the window itself must
        // stay (almost) fully on-screen - the limit follows the real character size.
        let big = CharacterZone {
            top: 0.05,
            bottom: 0.98,
        };
        let s = desktop();
        let p = plan_placement(&s, 900, -400, 640, 920, big, FitMode::KeepVisible);
        assert_eq!(head_y(&p, big), 0);
    }

    #[test]
    fn already_fitting_window_is_not_moved() {
        let s = desktop();
        let p = plan_placement(&s, 900, 100, 640, 920, Z, FitMode::KeepVisible);
        assert_eq!(
            p,
            Placement {
                x: 900,
                y: 100,
                w: 640,
                h: 920
            }
        );
    }

    #[test]
    fn character_dragged_below_taskbar_is_pulled_up() {
        let s = desktop();
        let p = plan_placement(&s, 900, 400, 640, 920, Z, FitMode::KeepVisible);
        assert_eq!(feet_y(&p, Z), s.work.bottom());
    }

    #[test]
    fn transparent_side_padding_may_hang_off_screen() {
        // A quarter of the width (160px) may be off-screen so the character can sit near the edge...
        let s = desktop();
        assert_eq!(
            plan_placement(&s, -150, 100, 640, 920, Z, FitMode::KeepVisible).x,
            -150
        );
        assert_eq!(
            plan_placement(&s, 1920 - 640 + 150, 100, 640, 920, Z, FitMode::KeepVisible).x,
            1920 - 640 + 150
        );
        // ...but not more, or the character itself would leave the screen.
        assert_eq!(
            plan_placement(&s, -500, 100, 640, 920, Z, FitMode::KeepVisible).x,
            -160
        );
        assert_eq!(
            plan_placement(&s, 1800, 100, 640, 920, Z, FitMode::KeepVisible).x,
            1920 - 640 + 160
        );
    }

    #[test]
    fn shrinking_keeps_bottom_centre_in_place() {
        // Window moved from a big monitor onto the laptop: same bottom edge & centre if possible.
        let s = screen(0, 0, 1366, 768, 60, 1.0, true); // 100%: wants 640x920, work height 708
        let p = plan_placement(&s, 300, -212, 640, 920, Z, FitMode::KeepVisible); // bottom at 708
        assert_eq!((p.w, p.h), (640, 708));
        assert_eq!(p.y as i64 + p.h as i64, 708);
        assert_eq!(p.x, 300);
    }

    #[test]
    fn work_area_offset_by_top_taskbar_is_respected() {
        let s = Screen {
            bounds: Rect {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            work: Rect {
                x: 0,
                y: 40,
                w: 1920,
                h: 1040,
            },
            scale: 1.0,
            primary: true,
        };
        // Head would be under a 40px top taskbar if the window went higher.
        let p = plan_placement(&s, 500, -600, 640, 920, Z, FitMode::KeepVisible);
        assert_eq!(head_y(&p, Z), 40);
    }

    #[test]
    fn picks_monitor_under_the_character_not_the_window_top_left() {
        let left = screen(-1920, 0, 1920, 1080, 40, 1.0, false);
        let main = desktop();
        // Window top-left on the left monitor, but the character (bottom-centre) on the main one.
        let picked = pick_screen(&[left, main], -200, 100, 640, 920).unwrap();
        assert_eq!(picked.bounds.x, 0);
    }

    #[test]
    fn falls_back_to_primary_when_window_is_on_a_disconnected_monitor() {
        let main = laptop_125();
        let picked = pick_screen(&[main], 3000, 200, 640, 920).unwrap();
        assert!(picked.primary);
        // And the resulting placement is on that screen.
        let p = plan_placement(&picked, 3000, 200, 640, 920, Z, FitMode::KeepVisible);
        assert!(p.x as i64 + p.w as i64 / 2 < main.work.right());
    }

    #[test]
    fn tiny_screen_never_produces_negative_or_inverted_ranges() {
        // Smaller than the window in both directions (e.g. a portrait 600x500 virtual display).
        let s = screen(0, 0, 600, 500, 40, 1.0, true);
        let p = plan_placement(&s, -9999, 9999, 640, 920, Z, FitMode::KeepVisible);
        assert_eq!((p.w, p.h), (600, 460));
        assert!(feet_y(&p, Z) <= s.work.bottom());
        assert!(head_y(&p, Z) >= 0);
    }

    // --- SizeOnly (after a user drag) & bring-to-centre ----------------------------------

    #[test]
    fn user_drag_is_never_moved_even_when_only_the_face_peeks_out() {
        // Reported use case: drag the character down so only the face/upper body shows above
        // the taskbar, or flick it away to hide it. The app must not pull it back.
        let s = desktop();
        for y in [700, 900, -800] {
            let p = plan_placement(&s, 900, y, 640, 920, Z, FitMode::SizeOnly);
            assert_eq!((p.x, p.y), (900, y));
        }
        let p = plan_placement(&s, -600, 0, 640, 920, Z, FitMode::SizeOnly);
        assert_eq!(p.x, -600);
    }

    #[test]
    fn size_only_still_fits_window_to_a_smaller_monitor_keeping_bottom_centre() {
        let s = laptop_125();
        let p = plan_placement(&s, 100, 50, 800, 1150, Z, FitMode::SizeOnly);
        assert_eq!((p.w, p.h), (800, 708));
        assert_eq!(p.y as i64 + p.h as i64, 50 + 1150);
    }

    #[test]
    fn bring_to_centre_puts_character_in_the_middle_of_the_work_area() {
        for s in [desktop(), laptop_125(), laptop_150_hidpi()] {
            let p = center_placement(&s, Z);
            let char_mid = (head_y(&p, Z) + feet_y(&p, Z)) / 2;
            let work_mid = s.work.y as i64 + s.work.h as i64 / 2;
            assert!(
                (char_mid - work_mid).abs() <= 2,
                "{s:?}: {char_mid} vs {work_mid}"
            );
            let win_mid = p.x as i64 + p.w as i64 / 2;
            let work_mid_x = s.work.x as i64 + s.work.w as i64 / 2;
            assert!((win_mid - work_mid_x).abs() <= 1);
            assert!(head_y(&p, Z) >= s.work.y as i64 && feet_y(&p, Z) <= s.work.bottom());
        }
    }

    #[test]
    fn bring_to_centre_works_on_a_secondary_monitor() {
        let left = screen(-1920, 0, 1920, 1080, 40, 1.0, false);
        let p = center_placement(&left, Z);
        assert!(p.x < 0 && p.x as i64 + p.w as i64 <= 0);
    }
}
