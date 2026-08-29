mod cursor_watcher;
mod session_watcher;

use tauri::{AppHandle, Manager, PhysicalSize};

/// Toggles whether this window ignores mouse events (click-through).
#[tauri::command]
fn set_click_through(window: tauri::Window, ignore: bool) -> Result<(), String> {
    window.set_ignore_cursor_events(ignore).map_err(|e| e.to_string())
}

/// Toggles always-on-top state.
#[tauri::command]
fn set_always_on_top(window: tauri::Window, always_on_top: bool) -> Result<(), String> {
    window.set_always_on_top(always_on_top).map_err(|e| e.to_string())
}

/// Dynamically resizes the window based on scale factor.
#[tauri::command]
fn set_window_size(window: tauri::Window, width: u32, height: u32) -> Result<(), String> {
    window.set_size(PhysicalSize::new(width, height)).map_err(|e| e.to_string())
}

/// Gracefully exits the application.
#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            set_click_through,
            set_always_on_top,
            set_window_size,
            exit_app
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                session_watcher::run(app_handle).await;
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
