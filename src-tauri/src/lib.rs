mod agent;
mod characters;
mod config;
mod cursor_watcher;
mod desktop;
mod live2d_import;
mod session_watcher;
#[cfg(test)]
mod test_util;
mod window_fit;

use tauri::{AppHandle, Emitter, Manager};

/// Toggles whether this window ignores mouse events (click-through).
#[tauri::command]
fn set_click_through(window: tauri::Window, ignore: bool) -> Result<(), String> {
    window
        .set_ignore_cursor_events(ignore)
        .map_err(|e| e.to_string())
}

/// Toggles always-on-top state.
#[tauri::command]
fn set_always_on_top(window: tauri::Window, always_on_top: bool) -> Result<(), String> {
    window
        .set_always_on_top(always_on_top)
        .map_err(|e| e.to_string())
}

/// Exits the application.
pub(crate) fn shutdown(app: &AppHandle) {
    app.exit(0);
}

/// Gracefully exits the application.
#[tauri::command]
fn exit_app(app: AppHandle) {
    shutdown(&app);
}

/// Opens or focuses the pre-warmed character settings manager window.
#[tauri::command]
fn open_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(settings_win) = app.get_webview_window("settings") {
        settings_win.show().map_err(|e| e.to_string())?;
        settings_win.unminimize().map_err(|e| e.to_string())?;
        settings_win.set_focus().map_err(|e| e.to_string())?;
        let _ = app.emit("characters-updated", ());
    } else {
        let _win = tauri::WebviewWindowBuilder::new(
            &app,
            "settings",
            tauri::WebviewUrl::App("settings.html".into()),
        )
        .title("ScreenMate - 캐릭터 매니저")
        .inner_size(680.0, 640.0)
        .min_inner_size(580.0, 500.0)
        .resizable(true)
        .decorations(true)
        .always_on_top(false)
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            set_click_through,
            set_always_on_top,
            exit_app,
            open_settings_window,
            characters::get_character_configs,
            characters::set_active_character,
            characters::save_2d_character,
            characters::delete_custom_character,
            characters::save_bubble_settings,
            characters::trigger_test_bubble,
            live2d_import::pick_folder,
            live2d_import::parse_live2d_folder,
            live2d_import::save_live2d_character,
            agent::get_agent_status,
            agent::get_custom_watch_paths,
            agent::add_custom_watch_path,
            agent::remove_custom_watch_path,
            agent::replay_fixture_session,
            desktop::hide_to_tray,
            desktop::sync_tray_state,
            window_fit::report_character_zone,
            window_fit::recenter_main,
            cursor_watcher::begin_drag,
            cursor_watcher::end_drag
        ])
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "settings" => {
                api.prevent_close();
                let _ = window.hide();
            }
            tauri::WindowEvent::Moved(_) if window.label() == "main" => {
                desktop::note_main_moved();
            }
            // Dragged onto a monitor with a different DPI scale: refit once things settle.
            tauri::WindowEvent::ScaleFactorChanged { .. } if window.label() == "main" => {
                desktop::note_main_moved();
            }
            _ => {}
        })
        .setup(|app| {
            // Set window icon explicitly for taskbar and window headers
            let icon = tauri::include_image!("icons/128x128.png");
            if let Some(main_win) = app.get_webview_window("main") {
                let _ = main_win.set_icon(icon.clone());
            }
            if let Some(settings_win) = app.get_webview_window("settings") {
                let _ = settings_win.set_icon(icon);
            }

            // System tray (the overlay is hidden from the taskbar). If the tray cannot be created
            // on this platform, fall back to a taskbar entry so the app is never unreachable.
            if let Err(e) = desktop::setup_tray(app.handle()) {
                eprintln!("[desktop] tray unavailable ({e}); showing taskbar entry instead");
                if let Some(main_win) = app.get_webview_window("main") {
                    let _ = main_win.set_skip_taskbar(false);
                }
            }

            // Start at the centre of the primary monitor (window fitted to it), then show it
            // (main starts hidden to avoid a visible jump).
            if let Some(main_win) = app.get_webview_window("main") {
                desktop::place_main_at_startup(&main_win);
                let _ = main_win.show();
            }
            tauri::async_runtime::spawn(desktop::run_move_watcher(app.handle().clone()));

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                session_watcher::run(app_handle, agent::custom_watch_paths).await;
            });

            let main_window = app
                .get_webview_window("main")
                .expect("main window must exist at setup time");
            tauri::async_runtime::spawn(async move {
                cursor_watcher::run(main_window).await;
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running screen-mate");
}
