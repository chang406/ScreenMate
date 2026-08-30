mod cursor_watcher;
mod session_watcher;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

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

/// Gracefully exits the application.
#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

/// Finds the `src` directory containing `characters.json`.
fn get_src_dir() -> Result<PathBuf, String> {
    let current = std::env::current_dir().map_err(|e| e.to_string())?;
    
    // Check possible relative paths
    let candidates = [
        current.join("src"),
        current.join("..").join("src"),
        PathBuf::from("src"),
        PathBuf::from("../src"),
    ];

    for candidate in &candidates {
        if candidate.join("characters.json").exists() {
            return Ok(candidate.canonicalize().unwrap_or_else(|_| candidate.clone()));
        }
    }

    // Default fallback
    if current.join("src").exists() {
        Ok(current.join("src"))
    } else {
        Ok(current.join("..").join("src"))
    }
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
        .always_on_top(true)
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Gets the parsed content of `src/characters.json`.
#[tauri::command]
fn get_character_configs() -> Result<Value, String> {
    let src_dir = get_src_dir()?;
    let json_path = src_dir.join("characters.json");
    if !json_path.exists() {
        return Err("characters.json not found".into());
    }
    let content = fs::read_to_string(&json_path).map_err(|e| e.to_string())?;
    let val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    Ok(val)
}

/// Sets the active character and notifies the main window.
#[tauri::command]
fn set_active_character(app: AppHandle, id: String) -> Result<(), String> {
    let src_dir = get_src_dir()?;
    let json_path = src_dir.join("characters.json");
    let content = fs::read_to_string(&json_path).map_err(|e| e.to_string())?;
    let mut val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    if let Some(obj) = val.as_object_mut() {
        obj.insert("activeCharacter".to_string(), Value::String(id.clone()));
    }

    let updated_json = serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?;
    fs::write(&json_path, updated_json).map_err(|e| e.to_string())?;

    // Notify main overlay window
    let _ = app.emit("switch-character", &id);
    let _ = app.emit("characters-updated", ());

    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Save2DCharacterPayload {
    pub id: String,
    pub name: String,
    pub files: HashMap<String, String>, // stateName -> DataURL or base64
    pub subtitles: HashMap<String, Vec<String>>,
}

/// Saves a custom 2D image character to custom_characters/<id> and registers it in characters.json.
#[tauri::command]
fn save_2d_character(app: AppHandle, payload: Save2DCharacterPayload) -> Result<(), String> {
    let src_dir = get_src_dir()?;
    let custom_char_dir = src_dir.join("characters").join("custom_characters").join(&payload.id);
    fs::create_dir_all(&custom_char_dir).map_err(|e| e.to_string())?;

    let mut states_map: HashMap<String, Value> = HashMap::new();

    for (state, data_url) in payload.files {
        // Parse Data URL: "data:image/png;base64,..." or "data:image/svg+xml;base64,..."
        let (ext, raw_bytes) = if let Some(comma_pos) = data_url.find(',') {
            let header = &data_url[..comma_pos];
            let base64_data = &data_url[comma_pos + 1..];
            let extension = if header.contains("svg") {
                "svg"
            } else if header.contains("gif") {
                "gif"
            } else if header.contains("webp") {
                "webp"
            } else if header.contains("jpeg") || header.contains("jpg") {
                "jpg"
            } else {
                "png"
            };

            let bytes = BASE64_STANDARD.decode(base64_data).map_err(|e| format!("Base64 decode error: {}", e))?;
            (extension, bytes)
        } else {
            // Raw text or fallback
            ("png", data_url.as_bytes().to_vec())
        };

        let file_name = format!("{}.{}", state, ext);
        let file_path = custom_char_dir.join(&file_name);
        fs::write(&file_path, raw_bytes).map_err(|e| e.to_string())?;

        let relative_src = format!("characters/custom_characters/{}/{}", payload.id, file_name);
        let subs = payload.subtitles.get(&state).cloned().unwrap_or_else(|| {
            match state.as_str() {
                "idle" => vec!["대기 중이에요~".to_string()],
                "thinking" => vec!["생각 중이에요...".to_string()],
                "talking" => vec!["작업 완료! ✨".to_string()],
                _ => vec!["대기 중".to_string()],
            }
        });

        states_map.insert(
            state,
            json!({
                "src": relative_src,
                "subtitles": subs
            }),
        );
    }

    // Update characters.json
    let json_path = src_dir.join("characters.json");
    let content = fs::read_to_string(&json_path).map_err(|e| e.to_string())?;
    let mut val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    if let Some(chars) = val.get_mut("characters").and_then(|c| c.as_object_mut()) {
        chars.insert(
            payload.id.clone(),
            json!({
                "name": payload.name,
                "type": "image",
                "states": states_map
            }),
        );
    }

    let updated_json = serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?;
    fs::write(&json_path, updated_json).map_err(|e| e.to_string())?;

    let _ = app.emit("characters-updated", ());
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Live2DParseResult {
    pub model_filename: String,
    pub model_folder: String,
    pub motion_groups: Vec<String>,
    pub suggested_mapping: HashMap<String, String>,
}

/// Recursively copies a directory to destination.
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), String> {
    fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name())).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Parses a Live2D folder to find model3.json and motion groups.
#[tauri::command]
fn parse_live2d_folder(folder_path: String) -> Result<Live2DParseResult, String> {
    let p = Path::new(&folder_path);
    if !p.exists() || !p.is_dir() {
        return Err("유효한 폴더 경로가 아닙니다.".into());
    }

    // Look for *.model3.json in the directory
    let mut model_file: Option<PathBuf> = None;
    for entry in fs::read_dir(p).map_err(|e| e.to_string())? {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".model3.json") {
                        model_file = Some(path);
                        break;
                    }
                }
            }
        }
    }

    let model_file_path = model_file.ok_or_else(|| "폴더 안에 *.model3.json 파일이 없습니다.".to_string())?;
    let model_filename = model_file_path.file_name().unwrap().to_str().unwrap().to_string();

    let content = fs::read_to_string(&model_file_path).map_err(|e| e.to_string())?;
    let json_val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let mut motion_groups: Vec<String> = Vec::new();
    if let Some(motions) = json_val
        .get("FileReferences")
        .and_then(|f| f.get("Motions"))
        .and_then(|m| m.as_object())
    {
        for key in motions.keys() {
            motion_groups.push(key.clone());
        }
    }

    // Auto-detect heuristic mapping
    let find_matching = |keywords: &[&str], fallback: &str| -> String {
        for group in &motion_groups {
            let lower = group.to_lowercase();
            for kw in keywords {
                if lower.contains(kw) {
                    return group.clone();
                }
            }
        }
        if !fallback.is_empty() && motion_groups.contains(&fallback.to_string()) {
            return fallback.to_string();
        }
        motion_groups.first().cloned().unwrap_or_default()
    };

    let idle_group = find_matching(&["idle", "stand", "wait", "default"], "");
    let think_group = find_matching(&["think", "look", "search", "question", "nod"], &idle_group);
    let busy_group = find_matching(&["busy", "work", "act", "magic", "skill", "attack"], &think_group);
    let talk_group = find_matching(&["talk", "speak", "cheer", "win", "joy", "happy"], &idle_group);
    let click_group = find_matching(&["tap", "touch", "click", "react", "talk"], &talk_group);
    let error_group = find_matching(&["error", "fail", "bad", "sad", "shock"], &think_group);
    let sleep_group = find_matching(&["sleep", "rest", "down"], &idle_group);

    let mut mapping = HashMap::new();
    mapping.insert("idle".to_string(), idle_group);
    mapping.insert("thinking".to_string(), think_group);
    mapping.insert("busy".to_string(), busy_group);
    mapping.insert("talking".to_string(), talk_group);
    mapping.insert("click_react".to_string(), click_group);
    mapping.insert("error".to_string(), error_group);
    mapping.insert("sleeping".to_string(), sleep_group);

    Ok(Live2DParseResult {
        model_filename,
        model_folder: folder_path,
        motion_groups,
        suggested_mapping: mapping,
    })
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveLive2DCharacterPayload {
    pub id: String,
    pub name: String,
    pub source_folder: String,
    pub model_filename: String,
    pub reactions: Value, // formatted reactions object
}

/// Imports and copies a Live2D character to custom_characters/<id> and registers it.
#[tauri::command]
fn save_live2d_character(app: AppHandle, payload: SaveLive2DCharacterPayload) -> Result<(), String> {
    let src_dir = get_src_dir()?;
    let custom_char_dir = src_dir.join("characters").join("custom_characters").join(&payload.id);
    
    // Copy all files from source directory
    copy_dir_all(&payload.source_folder, &custom_char_dir)?;

    let relative_model_path = format!(
        "characters/custom_characters/{}/{}",
        payload.id, payload.model_filename
    );

    // Update characters.json
    let json_path = src_dir.join("characters.json");
    let content = fs::read_to_string(&json_path).map_err(|e| e.to_string())?;
    let mut val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    if let Some(chars) = val.get_mut("characters").and_then(|c| c.as_object_mut()) {
        chars.insert(
            payload.id.clone(),
            json!({
                "name": payload.name,
                "type": "live2d",
                "modelPath": relative_model_path,
                "reactions": payload.reactions
            }),
        );
    }

    let updated_json = serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?;
    fs::write(&json_path, updated_json).map_err(|e| e.to_string())?;

    let _ = app.emit("characters-updated", ());
    Ok(())
}

/// Deletes a custom character from characters.json and removes its directory.
#[tauri::command]
fn delete_custom_character(app: AppHandle, id: String) -> Result<(), String> {
    if id == "mao" || id == "pixel_cat" || id == "wanko" {
        return Err("기본 프리셋 캐릭터는 삭제할 수 없습니다.".into());
    }

    let src_dir = get_src_dir()?;
    let custom_char_dir = src_dir.join("characters").join("custom_characters").join(&id);

    if custom_char_dir.exists() {
        let _ = fs::remove_dir_all(&custom_char_dir);
    }

    let json_path = src_dir.join("characters.json");
    let content = fs::read_to_string(&json_path).map_err(|e| e.to_string())?;
    let mut val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let mut need_switch = false;
    if let Some(active) = val.get("activeCharacter").and_then(|a| a.as_str()) {
        if active == id {
            need_switch = true;
        }
    }

    if let Some(chars) = val.get_mut("characters").and_then(|c| c.as_object_mut()) {
        chars.remove(&id);
    }

    if need_switch {
        if let Some(obj) = val.as_object_mut() {
            obj.insert("activeCharacter".to_string(), Value::String("mao".to_string()));
        }
        let _ = app.emit("switch-character", "mao");
    }

    let updated_json = serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?;
    fs::write(&json_path, updated_json).map_err(|e| e.to_string())?;

    let _ = app.emit("characters-updated", ());
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
            get_character_configs,
            set_active_character,
            save_2d_character,
            parse_live2d_folder,
            save_live2d_character,
            delete_custom_character
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "settings" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
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
