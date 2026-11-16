//! Desktop-resident integration: system tray menu and main-window placement.
//!
//! - Tray: show/hide character, open character manager, always-on-top / position-lock toggles,
//!   "bring to screen centre", quit. Toggles that the overlay owns (always-on-top, position lock)
//!   are forwarded to the main window as `tray-action` events so the overlay stays the single
//!   source of truth; it reports back via `sync_tray_state` so the check marks always match.
//! - Placement: the app always starts at the centre of the primary monitor (user decision; the
//!   previous "remember last position" behaviour was removed). After the user drags the
//!   character, its position is never changed - only the window size is re-fitted by
//!   [`crate::window_fit`] when it lands on a monitor of a different size / scale.

use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow, Wry};

/// How long the window must stay still before a move counts as "settled".
const MOVE_SETTLE: Duration = Duration::from_millis(800);
/// The first character-size report after start re-centres once (the character's real size is
/// only known after it has been rendered), unless it arrives later than this.
const STARTUP_RECENTER_WINDOW: Duration = Duration::from_secs(8);

/// Menu handles kept so their label / check state can be updated later.
pub struct TrayHandles {
    toggle_visible: MenuItem<Wry>,
    always_on_top: CheckMenuItem<Wry>,
    position_lock: CheckMenuItem<Wry>,
}

/// When the main window last moved (cleared once handled).
static PENDING_MOVE: Mutex<Option<Instant>> = Mutex::new(None);
/// App start time while the one-time "re-centre with the real character size" is still pending.
static STARTUP_RECENTER: Mutex<Option<Instant>> = Mutex::new(None);

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
    let reset_position = MenuItem::with_id(
        app,
        "reset_position",
        "캐릭터 화면 가운데로 불러오기",
        true,
        None::<&str>,
    )?;
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
            &reset_position,
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
            "reset_position" => {
                if let Some(win) = app.get_webview_window("main") {
                    crate::window_fit::recenter(&win, true);
                }
                set_main_visible(app, true);
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
// Placement
// ---------------------------------------------------------------------------

/// Startup placement: always the centre of the primary monitor.
pub fn place_main_at_startup(win: &WebviewWindow) {
    crate::window_fit::recenter(win, true);
    if let Ok(mut pending) = STARTUP_RECENTER.lock() {
        *pending = Some(Instant::now());
    }
}

/// Called when the overlay first reports the real character size: centre once more with it
/// (the startup centring used an estimate). Later reports are ignored here.
pub fn on_character_measured<R: Runtime>(win: &WebviewWindow<R>) {
    let started = STARTUP_RECENTER.lock().ok().and_then(|mut p| p.take());
    if started.is_some_and(|t| t.elapsed() < STARTUP_RECENTER_WINDOW) {
        crate::window_fit::recenter(win, true);
    }
}

/// Records a main-window move; handled by [`run_move_watcher`] once it settles.
pub fn note_main_moved() {
    if let Ok(mut pending) = PENDING_MOVE.lock() {
        *pending = Some(Instant::now());
    }
}

/// Background task: once a drag (or DPI / monitor change) has settled, fits the window *size* to
/// the monitor it landed on. The position chosen by the user is never changed (peeking from the
/// screen edge or hiding the character is intended use; "bring to centre" is the way back).
pub async fn run_move_watcher(app: AppHandle) {
    loop {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let settled = PENDING_MOVE
            .lock()
            .ok()
            .map(|mut p| match *p {
                Some(at) if at.elapsed() >= MOVE_SETTLE => {
                    *p = None;
                    true
                }
                _ => false,
            })
            .unwrap_or(false);
        if settled {
            if let Some(win) = app.get_webview_window("main") {
                crate::window_fit::refit_main(&win, crate::window_fit::FitMode::SizeOnly);
            }
        }
    }
}
