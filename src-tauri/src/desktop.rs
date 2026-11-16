//! Desktop-resident integration: system tray menu and main-window position persistence.
//!
//! - Tray: show/hide character, open character manager, always-on-top / position-lock toggles, quit.
//!   Toggles that the overlay owns (always-on-top, position lock) are forwarded to the main window as
//!   `tray-action` events so the overlay stays the single source of truth; it reports back via
//!   `sync_tray_state` so the check marks always match.
//! - Position: the main window's position is saved (debounced) into `app_config.json` as
//!   `windowPosition` and restored at startup only if it is still on a connected monitor.

use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, Runtime, WebviewWindow, Wry};

const POSITION_SAVE_DEBOUNCE: Duration = Duration::from_millis(800);

/// Menu handles kept so their label / check state can be updated later.
pub struct TrayHandles {
    toggle_visible: MenuItem<Wry>,
    always_on_top: CheckMenuItem<Wry>,
    position_lock: CheckMenuItem<Wry>,
}

/// Latest unsaved main-window position (physical px) and when it was observed.
static PENDING_POSITION: Mutex<Option<(i32, i32, Instant)>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Tray
// ---------------------------------------------------------------------------

pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let toggle_visible =
        MenuItem::with_id(app, "toggle_visible", "캐릭터 숨기기", true, None::<&str>)?;
    let open_settings = MenuItem::with_id(
        app,
        "open_settings",
        "캐릭터 매니저 열기",
        true,
        None::<&str>,
    )?;
    let always_on_top = CheckMenuItem::with_id(
        app,
        "always_on_top",
        "항상 위에 고정",
        true,
        true,
        None::<&str>,
    )?;
    let position_lock =
        CheckMenuItem::with_id(app, "position_lock", "위치 잠금", true, false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &toggle_visible,
            &open_settings,
            &PredefinedMenuItem::separator(app)?,
            &always_on_top,
            &position_lock,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let mut builder = TrayIconBuilder::with_id("screenmate-tray")
        .tooltip("ScreenMate")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle_visible" => toggle_main_visibility(app),
            "open_settings" => {
                let _ = crate::open_settings_window(app.clone());
            }
            "always_on_top" => {
                let _ = app.emit_to("main", "tray-action", "toggle_always_on_top");
            }
            "position_lock" => {
                let _ = app.emit_to("main", "tray-action", "toggle_position_lock");
            }
            "quit" => crate::shutdown(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_visibility(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;

    app.manage(TrayHandles {
        toggle_visible,
        always_on_top,
        position_lock,
    });
    Ok(())
}

fn refresh_visibility_label<R: Runtime>(app: &AppHandle<R>, visible: bool) {
    if let Some(handles) = app.try_state::<TrayHandles>() {
        let _ = handles.toggle_visible.set_text(if visible {
            "캐릭터 숨기기"
        } else {
            "캐릭터 보이기"
        });
    }
}

pub fn set_main_visible<R: Runtime>(app: &AppHandle<R>, visible: bool) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = if visible { win.show() } else { win.hide() };
        refresh_visibility_label(app, visible);
    }
}

fn toggle_main_visibility<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        let visible = win.is_visible().unwrap_or(true);
        set_main_visible(app, !visible);
    }
}

/// Hides the overlay to the tray (from the overlay's own context menu).
#[tauri::command]
pub fn hide_to_tray(app: AppHandle) {
    set_main_visible(&app, false);
}

/// Called by the overlay whenever its toggles change so the tray check marks match.
#[tauri::command]
pub fn sync_tray_state(app: AppHandle, always_on_top: bool, position_locked: bool) {
    if let Some(handles) = app.try_state::<TrayHandles>() {
        let _ = handles.always_on_top.set_checked(always_on_top);
        let _ = handles.position_lock.set_checked(position_locked);
    }
}

// ---------------------------------------------------------------------------
// Window position persistence
// ---------------------------------------------------------------------------

/// A monitor work area in physical pixels: (x, y, width, height).
pub type MonitorRect = (i32, i32, u32, u32);

/// True if the character area of a window at `pos` with `size` would be on one of `monitors`.
///
/// The overlay is a large transparent window whose character sits at the bottom centre, so we test
/// that anchor point (not the window's top-left corner, which may legitimately be off-screen).
pub fn is_position_visible(pos: (i32, i32), size: (u32, u32), monitors: &[MonitorRect]) -> bool {
    let anchor_x = pos.0 as i64 + size.0 as i64 / 2;
    let anchor_y = pos.1 as i64 + (size.1 as i64 * 4) / 5;
    monitors.iter().any(|&(mx, my, mw, mh)| {
        let (mx, my) = (mx as i64, my as i64);
        anchor_x >= mx && anchor_x < mx + mw as i64 && anchor_y >= my && anchor_y < my + mh as i64
    })
}

/// Parses `windowPosition` from the app config.
pub fn saved_position(config: &Value) -> Option<(i32, i32)> {
    let pos = config.get("windowPosition")?;
    let x = pos.get("x")?.as_i64()?;
    let y = pos.get("y")?.as_i64()?;
    Some((i32::try_from(x).ok()?, i32::try_from(y).ok()?))
}

/// Moves the main window to its saved position if that position is still on a connected monitor.
pub fn restore_main_position(win: &WebviewWindow, config: &Value) {
    let Some((x, y)) = saved_position(config) else {
        return;
    };
    let size = win
        .outer_size()
        .map(|s| (s.width, s.height))
        .unwrap_or((640, 920));
    let monitors: Vec<MonitorRect> = win
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let p = m.position();
            let s = m.size();
            (p.x, p.y, s.width, s.height)
        })
        .collect();
    if is_position_visible((x, y), size, &monitors) {
        let _ = win.set_position(PhysicalPosition::new(x, y));
    } else {
        println!("[desktop] saved position ({x}, {y}) is off-screen; keeping default position");
    }
}

/// Records a main-window move; the actual write happens in [`run_position_saver`].
pub fn note_main_moved(x: i32, y: i32) {
    if let Ok(mut pending) = PENDING_POSITION.lock() {
        *pending = Some((x, y, Instant::now()));
    }
}

/// Writes any pending position immediately (used on quit).
pub fn flush_position() {
    let pending = PENDING_POSITION.lock().ok().and_then(|mut p| p.take());
    if let Some((x, y, _)) = pending {
        let result = crate::update_app_config(|cfg| {
            if let Some(obj) = cfg.as_object_mut() {
                obj.insert("windowPosition".into(), json!({ "x": x, "y": y }));
            }
        });
        if let Err(e) = result {
            eprintln!("[desktop] failed to save window position: {e}");
        }
    }
}

/// Background task: persists the window position once dragging has settled.
pub async fn run_position_saver() {
    loop {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let settled = PENDING_POSITION
            .lock()
            .ok()
            .and_then(|p| p.map(|(_, _, at)| at.elapsed() >= POSITION_SAVE_DEBOUNCE))
            .unwrap_or(false);
        if settled {
            flush_position();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: (u32, u32) = (640, 920);

    #[test]
    fn position_on_primary_monitor_is_visible() {
        let monitors = [(0, 0, 1920, 1080)];
        assert!(is_position_visible((1200, 150), WIN, &monitors));
    }

    #[test]
    fn position_on_disconnected_second_monitor_is_rejected() {
        // Saved while a monitor was attached to the right; now only the primary remains.
        let monitors = [(0, 0, 1920, 1080)];
        assert!(!is_position_visible((2500, 100), WIN, &monitors));
        // ...but accepted again once the second monitor is back.
        let both = [(0, 0, 1920, 1080), (1920, 0, 2560, 1440)];
        assert!(is_position_visible((2500, 100), WIN, &both));
    }

    #[test]
    fn window_top_may_be_above_screen_if_character_is_visible() {
        // A tall transparent window dragged up so its empty top part is off-screen.
        let monitors = [(0, 0, 1920, 1080)];
        assert!(is_position_visible((600, -500), WIN, &monitors));
        assert!(!is_position_visible((600, -900), WIN, &monitors));
    }

    #[test]
    fn monitor_left_of_primary_has_negative_coordinates() {
        let monitors = [(-1920, 0, 1920, 1080), (0, 0, 1920, 1080)];
        assert!(is_position_visible((-1500, 100), WIN, &monitors));
    }

    #[test]
    fn saved_position_parsing() {
        assert_eq!(
            saved_position(&json!({ "windowPosition": { "x": -10, "y": 20 } })),
            Some((-10, 20))
        );
        assert_eq!(
            saved_position(&json!({ "windowPosition": { "x": "a" } })),
            None
        );
        assert_eq!(saved_position(&json!({})), None);
    }
}
