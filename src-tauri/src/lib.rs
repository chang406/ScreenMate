mod cursor_watcher;
mod desktop;
mod session_watcher;
mod window_fit;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
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

/// Saves pending state (window position) and exits the application.
pub(crate) fn shutdown(app: &AppHandle) {
    desktop::flush_position();
    app.exit(0);
}

/// Gracefully exits the application.
#[tauri::command]
fn exit_app(app: AppHandle) {
    shutdown(&app);
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
        .always_on_top(false)
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn get_app_config_path(src_dir: &Path) -> PathBuf {
    src_dir.join("app_config.json")
}

fn load_app_config(src_dir: &Path) -> Value {
    let path = get_app_config_path(src_dir);
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(val) = serde_json::from_str::<Value>(&content) {
                return val;
            }
        }
    }
    // Default config if not present
    let default_config = json!({
        "activeCharacter": "mao",
        "customWatchPaths": [],
        "globalBubbleSettings": {
            "autoTrack": true,
            "bgColor": "#0f0f14",
            "bgOpacity": 0.92,
            "borderColor": "#404050",
            "borderRadius": 14,
            "duration": 3.5,
            "fontSize": 13,
            "hasTail": true,
            "offsetX": 0,
            "offsetY": 0
        }
    });
    let _ = fs::write(&path, serde_json::to_string_pretty(&default_config).unwrap_or_default());
    default_config
}

fn save_app_config(src_dir: &Path, config: &Value) -> Result<(), String> {
    let path = get_app_config_path(src_dir);
    let pretty = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, pretty).map_err(|e| e.to_string())
}

/// Serializes read-modify-write cycles on `app_config.json`. Several writers exist (settings
/// window commands, the debounced window-position saver), and without this lock two
/// concurrent updates could each load the old file and one change would be lost.
static APP_CONFIG_LOCK: Mutex<()> = Mutex::new(());

/// Loads `app_config.json`, applies `mutate`, and saves it back atomically w.r.t. other updaters.
pub(crate) fn update_app_config<F: FnOnce(&mut Value)>(mutate: F) -> Result<Value, String> {
    let _guard = APP_CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let src_dir = get_src_dir()?;
    let mut config = load_app_config(&src_dir);
    mutate(&mut config);
    save_app_config(&src_dir, &config)?;
    Ok(config)
}

/// Finds the `character.json` file for a given character ID.
/// Checks custom_characters first, then standard characters.
fn find_character_json_path(src_dir: &Path, char_id: &str) -> Option<PathBuf> {
    let custom_path = src_dir
        .join("characters")
        .join("custom_characters")
        .join(char_id)
        .join("character.json");
    if custom_path.exists() {
        return Some(custom_path);
    }
    let default_path = src_dir.join("characters").join(char_id).join("character.json");
    if default_path.exists() {
        return Some(default_path);
    }
    None
}

/// Scans character directories and merges them with app_config.json.
/// Note: The root `characters.json` is treated as a sample/template reference only and ignored here.
#[tauri::command]
fn get_character_configs() -> Result<Value, String> {
    let src_dir = get_src_dir()?;
    let mut app_config = load_app_config(&src_dir);

    let mut characters = serde_json::Map::new();

    // 1. Scan public characters in src/characters/<id>/character.json (skip custom_characters)
    let base_chars_dir = src_dir.join("characters");
    if base_chars_dir.exists() && base_chars_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&base_chars_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    if dir_name == "custom_characters" {
                        continue;
                    }
                    let char_json_path = path.join("character.json");
                    if char_json_path.exists() {
                        if let Ok(content) = fs::read_to_string(&char_json_path) {
                            if let Ok(char_val) = serde_json::from_str::<Value>(&content) {
                                characters.insert(dir_name, char_val);
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Scan custom characters in src/characters/custom_characters/<id>/character.json (skip _template)
    let custom_chars_dir = src_dir.join("characters").join("custom_characters");
    if custom_chars_dir.exists() && custom_chars_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&custom_chars_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let dir_name = entry.file_name().to_string_lossy().to_string();
                    if dir_name == "_template" {
                        continue;
                    }
                    let char_json_path = path.join("character.json");
                    if char_json_path.exists() {
                        if let Ok(content) = fs::read_to_string(&char_json_path) {
                            if let Ok(char_val) = serde_json::from_str::<Value>(&content) {
                                characters.insert(dir_name, char_val);
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Fallback check for activeCharacter
    let mut active = app_config
        .get("activeCharacter")
        .and_then(|v| v.as_str())
        .unwrap_or("mao")
        .to_string();

    if !characters.contains_key(&active) {
        if characters.contains_key("mao") {
            active = "mao".to_string();
        } else if let Some((first_key, _)) = characters.iter().next() {
            active = first_key.clone();
        }
        let fixed = active.clone();
        let _ = update_app_config(|cfg| {
            if let Some(obj) = cfg.as_object_mut() {
                obj.insert("activeCharacter".to_string(), Value::String(fixed));
            }
        });
        if let Some(obj) = app_config.as_object_mut() {
            obj.insert("activeCharacter".to_string(), Value::String(active.clone()));
        }
    }

    let global_bubble = app_config
        .get("globalBubbleSettings")
        .cloned()
        .unwrap_or_else(|| {
            json!({
                "autoTrack": true,
                "bgColor": "#0f0f14",
                "bgOpacity": 0.92,
                "borderColor": "#404050",
                "borderRadius": 14,
                "duration": 3.5,
                "fontSize": 13,
                "hasTail": true,
                "offsetX": 0,
                "offsetY": 0
            })
        });

    Ok(json!({
        "activeCharacter": active,
        "characters": characters,
        "globalBubbleSettings": global_bubble
    }))
}

/// Sets the active character and notifies the main window.
#[tauri::command]
fn set_active_character(app: AppHandle, id: String) -> Result<(), String> {
    update_app_config(|cfg| {
        if let Some(obj) = cfg.as_object_mut() {
            obj.insert("activeCharacter".to_string(), Value::String(id.clone()));
        }
    })?;

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

    // Save character.json inside custom_characters/<id>/
    let char_json = json!({
        "name": payload.name,
        "type": "image",
        "states": states_map
    });
    let char_json_path = custom_char_dir.join("character.json");
    let updated_json = serde_json::to_string_pretty(&char_json).map_err(|e| e.to_string())?;
    fs::write(&char_json_path, updated_json).map_err(|e| e.to_string())?;

    let _ = app.emit("characters-updated", ());
    Ok(())
}

/// Checks if a PNG texture file has 4K resolution (width or height >= 3840)
fn is_png_4k(path: &Path) -> bool {
    if let Ok(mut file) = fs::File::open(path) {
        use std::io::Read;
        let mut header = [0u8; 24];
        if file.read_exact(&mut header).is_ok() {
            // PNG signature: 89 50 4E 47 0D 0A 1A 0A
            if &header[0..8] == b"\x89PNG\r\n\x1a\n" {
                // IHDR width at 16..20, height at 20..24 (Big Endian)
                let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
                let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
                return width >= 3840 || height >= 3840;
            }
        }
    }
    false
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Live2DParseResult {
    pub model_filename: String,
    pub model_folder: String,
    pub motion_groups: Vec<String>,
    pub expressions: Vec<String>,
    pub suggested_mapping: HashMap<String, String>,
    pub suggested_expressions: HashMap<String, String>,
    pub is_4k: bool,
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

/// Opens a native folder selection dialog.
#[tauri::command]
fn pick_folder() -> Result<Option<String>, String> {
    let folder = rfd::FileDialog::new()
        .set_title("Live2D 모델 폴더 선택")
        .pick_folder();
    Ok(folder.map(|p| p.to_string_lossy().to_string()))
}

/// Recursively searches for *.model3.json up to max_depth levels.
fn find_model3_json(dir: &Path, max_depth: usize) -> Option<PathBuf> {
    if max_depth == 0 || !dir.is_dir() {
        return None;
    }

    if let Ok(entries) = fs::read_dir(dir) {
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".model3.json") {
                        return Some(path);
                    }
                }
            } else if path.is_dir() {
                subdirs.push(path);
            }
        }

        for subdir in subdirs {
            if let Some(found) = find_model3_json(&subdir, max_depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

/// Recursively searches for *.motion3.json files in a directory up to max_depth.
/// Returns a list of (group_name, relative_path_string).
fn find_motion_files(base_dir: &Path, current_dir: &Path, max_depth: usize) -> Vec<(String, String)> {
    let mut results = Vec::new();
    if max_depth == 0 || !current_dir.is_dir() {
        return results;
    }

    if let Ok(entries) = fs::read_dir(current_dir) {
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".motion3.json") {
                        let group_name = name.trim_end_matches(".motion3.json").to_string();
                        if let Ok(rel) = path.strip_prefix(base_dir) {
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            results.push((group_name, rel_str));
                        }
                    }
                }
            } else if path.is_dir() {
                subdirs.push(path);
            }
        }

        for subdir in subdirs {
            results.extend(find_motion_files(base_dir, &subdir, max_depth - 1));
        }
    }
    results
}

/// Recursively searches for *.exp3.json files in a directory up to max_depth.
/// Returns a list of (expression_name, relative_path_string).
fn find_expression_files(base_dir: &Path, current_dir: &Path, max_depth: usize) -> Vec<(String, String)> {
    let mut results = Vec::new();
    if max_depth == 0 || !current_dir.is_dir() {
        return results;
    }

    if let Ok(entries) = fs::read_dir(current_dir) {
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with(".exp3.json") {
                        let exp_name = name.trim_end_matches(".exp3.json").to_string();
                        if let Ok(rel) = path.strip_prefix(base_dir) {
                            let rel_str = rel.to_string_lossy().replace('\\', "/");
                            results.push((exp_name, rel_str));
                        }
                    }
                }
            } else if path.is_dir() {
                subdirs.push(path);
            }
        }

        for subdir in subdirs {
            results.extend(find_expression_files(base_dir, &subdir, max_depth - 1));
        }
    }
    results
}

/// Matches a candidate from list using multi-level multilingual keyword groups.
fn match_keyword_thesaurus(candidates: &[String], keyword_groups: &[&[&str]], fallback: &str) -> String {
    for group in keyword_groups {
        for cand in candidates {
            let lower = cand.to_lowercase();
            for kw in *group {
                if lower.contains(kw) {
                    return cand.clone();
                }
            }
        }
    }
    if !fallback.is_empty() && candidates.contains(&fallback.to_string()) {
        return fallback.to_string();
    }
    String::new()
}

/// Parses a Live2D folder to find model3.json, motion groups, and expressions.
#[tauri::command]
fn parse_live2d_folder(folder_path: String) -> Result<Live2DParseResult, String> {
    let p = Path::new(&folder_path);
    if !p.exists() || !p.is_dir() {
        return Err("유효한 폴더 경로가 아닙니다.".into());
    }

    // Look for *.model3.json in the directory (up to 3 levels deep)
    let model_file_path = find_model3_json(p, 3)
        .ok_or_else(|| "폴더 또는 하위 폴더 안에 *.model3.json 파일이 없습니다.".to_string())?;

    let model_filename = model_file_path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Actual directory containing the .model3.json
    let actual_model_dir = model_file_path.parent().unwrap_or(p);
    let actual_folder_str = actual_model_dir.to_string_lossy().to_string();

    let content = fs::read_to_string(&model_file_path).map_err(|e| e.to_string())?;
    let json_val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    // Check if any texture is 4K (width/height >= 3840)
    let mut is_4k = false;
    if let Some(textures) = json_val
        .get("FileReferences")
        .and_then(|f| f.get("Textures"))
        .and_then(|t| t.as_array())
    {
        for tex in textures {
            if let Some(rel_path) = tex.as_str() {
                let tex_full_path = actual_model_dir.join(rel_path);
                if is_png_4k(&tex_full_path) {
                    is_4k = true;
                    break;
                }
            }
        }
    }

    // 1. Collect motion groups
    let mut motion_groups: Vec<String> = Vec::new();
    let mut existing_motion_files: HashSet<String> = HashSet::new();

    if let Some(motions) = json_val
        .get("FileReferences")
        .and_then(|f| f.get("Motions"))
        .and_then(|m| m.as_object())
    {
        for (key, val) in motions {
            motion_groups.push(key.clone());
            if let Some(arr) = val.as_array() {
                for item in arr {
                    if let Some(file_str) = item.get("File").and_then(|f| f.as_str()) {
                        existing_motion_files.insert(file_str.replace('\\', "/"));
                    }
                }
            }
        }
    }

    // Scan directory for loose *.motion3.json files
    let loose_motions = find_motion_files(actual_model_dir, actual_model_dir, 3);
    for (group_name, rel_path) in loose_motions {
        let normalized = rel_path.replace('\\', "/");
        if !existing_motion_files.contains(&normalized) && !motion_groups.contains(&group_name) {
            motion_groups.push(group_name);
        }
    }

    // 2. Collect expressions
    let mut expressions: Vec<String> = Vec::new();
    let mut existing_exp_files: HashSet<String> = HashSet::new();

    if let Some(exps) = json_val
        .get("FileReferences")
        .and_then(|f| f.get("Expressions"))
        .and_then(|e| e.as_array())
    {
        for item in exps {
            if let Some(name) = item.get("Name").and_then(|n| n.as_str()) {
                if !expressions.contains(&name.to_string()) {
                    expressions.push(name.to_string());
                }
            }
            if let Some(file_str) = item.get("File").and_then(|f| f.as_str()) {
                existing_exp_files.insert(file_str.replace('\\', "/"));
            }
        }
    }

    // Scan directory for loose *.exp3.json files
    let loose_exps = find_expression_files(actual_model_dir, actual_model_dir, 3);
    for (exp_name, rel_path) in loose_exps {
        let normalized = rel_path.replace('\\', "/");
        if !existing_exp_files.contains(&normalized) && !expressions.contains(&exp_name) {
            expressions.push(exp_name);
        }
    }

    // 3. Motion Thesaurus (English, Korean, Japanese, Chinese)
    let idle_motion_kw: &[&[&str]] = &[
        &["idle", "stand", "wait", "default", "loop", "scene1", "normal", "main"],
        &["대기", "기본", "숨", "루프", "평상"],
        &["待機", "通常", "デフォルト", "ポーズ", "ループ"],
        &["待机", "默认", "平时", "循环"],
    ];
    let think_motion_kw: &[&[&str]] = &[
        &["think", "look", "search", "question", "nod", "haoqi", "wonder", "curious"],
        &["생각", "고민", "호기심", "분석", "탐색"],
        &["考える", "思考", "疑問", "見渡す", "頷く"],
        &["思考", "好奇", "疑问", "看看", "点头"],
    ];
    let busy_motion_kw: &[&[&str]] = &[
        &["busy", "work", "act", "magic", "skill", "attack", "zhentou", "scene"],
        &["작업", "집중", "코딩", "스킬", "공격", "베개", "일"],
        &["作業", "仕事", "攻撃", "スキル", "魔法", "枕"],
        &["工作", "干活", "枕头", "抱枕", "施法", "攻击"],
    ];
    let talk_motion_kw: &[&[&str]] = &[
        &["talk", "speak", "cheer", "win", "joy", "happy", "qizi", "smile", "done"],
        &["완료", "성공", "기쁨", "대화", "미소", "깃발", "말하기"],
        &["会話", "喜び", "勝利", "話す", "笑顔", "旗", "完了"],
        &["说话", "开心", "胜利", "聊天", "旗子", "完成"],
    ];
    let click_motion_kw: &[&[&str]] = &[
        &["tap", "touch", "click", "react", "yaotou", "poke", "hit"],
        &["터치", "클릭", "반응", "도리도리", "쓰담", "반기기"],
        &["タッチ", "クリック", "反応", "頭揺らし", "照れ", "つつく"],
        &["摇头", "点击", "触摸", "互动", "戳"],
    ];
    let error_motion_kw: &[&[&str]] = &[
        &["error", "fail", "bad", "sad", "shock", "linghun", "cry", "panic"],
        &["에러", "실패", "놀람", "영혼", "슬픔", "울음", "당황"],
        &["エラー", "失敗", "驚き", "ショック", "魂", "泣き", "困る"],
        &["灵魂", "失败", "错误", "难过", "震惊", "慌张"],
    ];
    let sleep_motion_kw: &[&[&str]] = &[
        &["sleep", "rest", "down", "keshui", "bed", "zzz", "lay"],
        &["수면", "잠", "휴식", "졸림", "꾸벅", "눕기"],
        &["睡眠", "寝る", "おやすみ", "休憩", "居眠り", "横になる"],
        &["瞌睡", "睡觉", "休息", "晚安", "躺下"],
    ];

    let idle_group = if motion_groups.is_empty() {
        String::new()
    } else {
        let matched = match_keyword_thesaurus(&motion_groups, idle_motion_kw, "");
        if matched.is_empty() {
            motion_groups.first().cloned().unwrap_or_default()
        } else {
            matched
        }
    };
    let think_group = match_keyword_thesaurus(&motion_groups, think_motion_kw, &idle_group);
    let busy_group = match_keyword_thesaurus(&motion_groups, busy_motion_kw, &think_group);
    let talk_group = match_keyword_thesaurus(&motion_groups, talk_motion_kw, &idle_group);
    let click_group = match_keyword_thesaurus(&motion_groups, click_motion_kw, &talk_group);
    let error_group = match_keyword_thesaurus(&motion_groups, error_motion_kw, &think_group);
    let sleep_group = match_keyword_thesaurus(&motion_groups, sleep_motion_kw, &idle_group);

    let mut mapping = HashMap::new();
    mapping.insert("idle".to_string(), idle_group);
    mapping.insert("thinking".to_string(), think_group);
    mapping.insert("busy".to_string(), busy_group);
    mapping.insert("talking".to_string(), talk_group);
    mapping.insert("click_react".to_string(), click_group);
    mapping.insert("error".to_string(), error_group);
    mapping.insert("sleeping".to_string(), sleep_group);

    // 4. Expression Thesaurus (English, Korean, Japanese, Chinese)
    let idle_exp_kw: &[&[&str]] = &[
        &["normal", "default", "idle", "standard"],
        &["기본", "평상시", "보통"],
        &["通常", "デフォルト", "普通"],
        &["平时", "默认", "正常"],
    ];
    let think_exp_kw: &[&[&str]] = &[
        &["star_eyes", "star", "think", "look", "question", "haoqi", "sparkle", "wonder"],
        &["별눈", "반짝", "생각", "호기심", "물음", "고민"],
        &["キラキラ", "星目", "考える", "疑問", "見つめる", "星"],
        &["星星眼", "星星", "好奇", "思考", "疑问", "闪亮"],
    ];
    let busy_exp_kw: &[&[&str]] = &[
        &["left_hand", "right_hand", "hand", "work", "busy", "baozhen", "focus", "serious"],
        &["손", "오른손", "왼손", "집중", "포옹", "진지"],
        &["右手", "左手", "手", "集中", "抱き枕", "真面目"],
        &["右手", "左手", "手", "抱枕", "专注", "认真"],
    ];
    let talk_exp_kw: &[&[&str]] = &[
        &["heart_eyes", "heart", "smile", "happy", "joy", "star_eyes", "qizi", "cheer"],
        &["하트눈", "하트", "미소", "기쁨", "웃음", "깃발", "행복"],
        &["ハート", "笑顔", "笑い", "喜び", "目ハート", "旗", "ハッピー"],
        &["爱心眼", "爱心", "笑", "旗子", "开心", "高兴"],
    ];
    let click_exp_kw: &[&[&str]] = &[
        &["blush", "heart_eyes", "shy", "smile", "heart", "react", "wink"],
        &["볼붉힘", "부끄", "하트", "미소", "홍조", "윙크"],
        &["照れ", "赤面", "恥ずかしい", "ハート", "笑顔", "ウインク"],
        &["脸红", "爱心眼", "害羞", "红晕", "眨眼"],
    ];
    let error_exp_kw: &[&[&str]] = &[
        &["angry", "tears", "cry", "dark_face", "white_eyes", "blood", "sad", "shock", "dizzy"],
        &["화남", "분노", "눈물", "울음", "어두움", "백안", "피", "당황", "어지러움"],
        &["怒り", "涙", "泣き", "暗い", "白目", "血", "青ざめ", "ショック"],
        &["生气", "泪", "白眼", "脸黑", "血", "哭", "怒", "震惊"],
    ];
    let sleep_exp_kw: &[&[&str]] = &[
        &["dark_face", "tears", "sleep", "rest", "cry", "drowsy", "closed"],
        &["어두움", "눈물", "수면", "잠", "졸림", "감은눈"],
        &["暗い", "涙", "眠い", "睡眠", "居眠り", "目を閉じる"],
        &["脸黑", "泪", "瞌睡", "睡觉", "闭眼"],
    ];

    let idle_exp = match_keyword_thesaurus(&expressions, idle_exp_kw, "");
    let think_exp = match_keyword_thesaurus(&expressions, think_exp_kw, "");
    let busy_exp = match_keyword_thesaurus(&expressions, busy_exp_kw, "");
    let talk_exp = match_keyword_thesaurus(&expressions, talk_exp_kw, "");
    let click_exp = match_keyword_thesaurus(&expressions, click_exp_kw, "");
    let error_exp = match_keyword_thesaurus(&expressions, error_exp_kw, "");
    let sleep_exp = match_keyword_thesaurus(&expressions, sleep_exp_kw, "");

    let mut exp_mapping = HashMap::new();
    exp_mapping.insert("idle".to_string(), idle_exp);
    exp_mapping.insert("thinking".to_string(), think_exp);
    exp_mapping.insert("busy".to_string(), busy_exp);
    exp_mapping.insert("talking".to_string(), talk_exp);
    exp_mapping.insert("click_react".to_string(), click_exp);
    exp_mapping.insert("error".to_string(), error_exp);
    exp_mapping.insert("sleeping".to_string(), sleep_exp);

    Ok(Live2DParseResult {
        model_filename,
        model_folder: actual_folder_str,
        motion_groups,
        expressions,
        suggested_mapping: mapping,
        suggested_expressions: exp_mapping,
        is_4k,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveLive2DCharacterPayload {
    pub id: String,
    pub name: String,
    #[serde(alias = "source_folder")]
    pub source_folder: String,
    #[serde(alias = "model_filename")]
    pub model_filename: String,
    pub reactions: Value, // formatted reactions object
    #[serde(alias = "is_4k")]
    pub is_4k: Option<bool>,
}

/// Sanitizes and normalizes Live2D filenames (e.g., non-ASCII, Chinese, Japanese) to ASCII ID-based filenames.
fn sanitize_live2d_filenames(custom_char_dir: &Path, id: &str, model_filename: &str) -> Result<String, String> {
    let old_model_path = custom_char_dir.join(model_filename);
    if !old_model_path.exists() {
        return Ok(model_filename.to_string());
    }

    let prefix = model_filename.trim_end_matches(".model3.json");
    let is_non_ascii = !model_filename.is_ascii() || prefix != id;

    if !is_non_ascii {
        return Ok(model_filename.to_string());
    }

    let new_model_filename = format!("{}.model3.json", id);
    let new_model_path = custom_char_dir.join(&new_model_filename);

    let content = fs::read_to_string(&old_model_path).map_err(|e| e.to_string())?;
    let mut model_json: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    if let Some(file_refs) = model_json.get_mut("FileReferences").and_then(|f| f.as_object_mut()) {
        if let Some(moc) = file_refs.get_mut("Moc").and_then(|m| m.as_str()) {
            let new_moc = moc.replace(prefix, id);
            file_refs.insert("Moc".to_string(), Value::String(new_moc));
        }
        if let Some(phy) = file_refs.get_mut("Physics").and_then(|m| m.as_str()) {
            let new_phy = phy.replace(prefix, id);
            file_refs.insert("Physics".to_string(), Value::String(new_phy));
        }
        if let Some(cdi) = file_refs.get_mut("DisplayInfo").and_then(|m| m.as_str()) {
            let new_cdi = cdi.replace(prefix, id);
            file_refs.insert("DisplayInfo".to_string(), Value::String(new_cdi));
        }
        if let Some(textures) = file_refs.get_mut("Textures").and_then(|t| t.as_array_mut()) {
            for tex in textures {
                if let Some(tex_str) = tex.as_str() {
                    let new_tex = tex_str.replace(prefix, id);
                    *tex = Value::String(new_tex);
                }
            }
        }
    }

    // Rename matching files and folders on disk
    if let Ok(entries) = fs::read_dir(custom_char_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with(prefix) && (name.len() == prefix.len() || name.chars().nth(prefix.len()) == Some('.')) {
                    let suffix = &name[prefix.len()..];
                    let new_name = format!("{}{}", id, suffix);
                    let new_path = custom_char_dir.join(new_name);
                    if path != new_path {
                        let _ = fs::rename(&path, &new_path);
                    }
                }
            }
        }
    }

    let pretty = serde_json::to_string_pretty(&model_json).map_err(|e| e.to_string())?;
    fs::write(&new_model_path, pretty).map_err(|e| e.to_string())?;

    if old_model_path != new_model_path && old_model_path.exists() {
        let _ = fs::remove_file(&old_model_path);
    }

    Ok(new_model_filename)
}

/// Imports and copies a Live2D character to custom_characters/<id> and registers it.
#[tauri::command]
fn save_live2d_character(app: AppHandle, payload: SaveLive2DCharacterPayload) -> Result<(), String> {
    let src_dir = get_src_dir()?;
    let custom_char_dir = src_dir.join("characters").join("custom_characters").join(&payload.id);
    
    // Copy all files from source directory
    copy_dir_all(&payload.source_folder, &custom_char_dir)?;

    // Sanitize non-ASCII / foreign character filenames to standard ASCII id-based filenames
    let effective_model_filename = sanitize_live2d_filenames(&custom_char_dir, &payload.id, &payload.model_filename)
        .unwrap_or_else(|_| payload.model_filename.clone());

    // Ensure model3.json has all loose motions and expressions registered
    let target_model_file = custom_char_dir.join(&effective_model_filename);
    if target_model_file.exists() {
        if let Ok(content) = fs::read_to_string(&target_model_file) {
            if let Ok(mut model_json) = serde_json::from_str::<Value>(&content) {
                let loose_motions = find_motion_files(&custom_char_dir, &custom_char_dir, 3);
                let loose_exps = find_expression_files(&custom_char_dir, &custom_char_dir, 3);
                let mut modified = false;

                if let Some(file_refs) = model_json.get_mut("FileReferences").and_then(|f| f.as_object_mut()) {
                    if !loose_motions.is_empty() {
                        if !file_refs.contains_key("Motions") || !file_refs["Motions"].is_object() {
                            file_refs.insert("Motions".to_string(), json!({}));
                        }
                        if let Some(motions_obj) = file_refs.get_mut("Motions").and_then(|m| m.as_object_mut()) {
                            for (group_name, rel_path) in loose_motions {
                                if !motions_obj.contains_key(&group_name) {
                                    motions_obj.insert(
                                        group_name,
                                        json!([
                                            { "File": rel_path }
                                        ])
                                    );
                                    modified = true;
                                }
                            }
                        }
                    }

                    if !loose_exps.is_empty() {
                        if !file_refs.contains_key("Expressions") || !file_refs["Expressions"].is_array() {
                            file_refs.insert("Expressions".to_string(), json!([]));
                        }
                        if let Some(exps_arr) = file_refs.get_mut("Expressions").and_then(|e| e.as_array_mut()) {
                            let existing_exp_names: HashSet<String> = exps_arr
                                .iter()
                                .filter_map(|e| e.get("Name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                                .collect();
                            for (exp_name, rel_path) in loose_exps {
                                if !existing_exp_names.contains(&exp_name) {
                                    exps_arr.push(json!({
                                        "Name": exp_name,
                                        "File": rel_path
                                    }));
                                    modified = true;
                                }
                            }
                        }
                    }
                }

                if modified {
                    if let Ok(pretty) = serde_json::to_string_pretty(&model_json) {
                        let _ = fs::write(&target_model_file, pretty);
                    }
                }
            }
        }
    }

    let relative_model_path = format!(
        "characters/custom_characters/{}/{}",
        payload.id, effective_model_filename
    );

    // Save character.json inside custom_characters/<id>/
    let is_4k_val = payload.is_4k.unwrap_or(false);
    let mut char_json = json!({
        "name": payload.name,
        "type": "live2d",
        "modelPath": relative_model_path,
        "reactions": payload.reactions
    });
    if is_4k_val {
        char_json["is4k"] = json!(true);
    }

    let char_json_path = custom_char_dir.join("character.json");
    let updated_json = serde_json::to_string_pretty(&char_json).map_err(|e| e.to_string())?;
    fs::write(&char_json_path, updated_json).map_err(|e| e.to_string())?;

    let _ = app.emit("characters-updated", ());
    Ok(())
}

/// Deletes a custom character directory.
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

    let mut need_switch = false;
    let _ = update_app_config(|cfg| {
        if cfg.get("activeCharacter").and_then(|a| a.as_str()) == Some(id.as_str()) {
            need_switch = true;
            if let Some(obj) = cfg.as_object_mut() {
                obj.insert("activeCharacter".to_string(), Value::String("mao".to_string()));
            }
        }
    });

    if need_switch {
        let _ = app.emit("switch-character", "mao");
    }

    let _ = app.emit("characters-updated", ());
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveBubbleSettingsPayload {
    #[serde(alias = "target_id")]
    pub target_id: Option<String>,
    pub settings: Value,
}

/// Saves global or character-specific speech bubble settings.
#[tauri::command]
fn save_bubble_settings(app: AppHandle, payload: SaveBubbleSettingsPayload) -> Result<(), String> {
    let src_dir = get_src_dir()?;

    if let Some(target) = payload.target_id.as_deref() {
        if target == "global" {
            update_app_config(|cfg| {
                if let Some(obj) = cfg.as_object_mut() {
                    obj.insert("globalBubbleSettings".to_string(), payload.settings);
                }
            })?;
        } else if let Some(char_json_path) = find_character_json_path(&src_dir, target) {
            let content = fs::read_to_string(&char_json_path).map_err(|e| e.to_string())?;
            let mut char_val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
            if let Some(obj) = char_val.as_object_mut() {
                obj.insert("bubbleSettings".to_string(), payload.settings);
            }
            let updated = serde_json::to_string_pretty(&char_val).map_err(|e| e.to_string())?;
            fs::write(&char_json_path, updated).map_err(|e| e.to_string())?;
        } else {
            return Err(format!("캐릭터 '{}'의 설정 파일을 찾을 수 없습니다.", target));
        }
    } else {
        update_app_config(|cfg| {
            if let Some(obj) = cfg.as_object_mut() {
                obj.insert("globalBubbleSettings".to_string(), payload.settings);
            }
        })?;
    }

    let _ = app.emit("bubble-settings-updated", ());
    let _ = app.emit("characters-updated", ());

    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TestBubblePayload {
    pub text: Option<String>,
    pub settings: Option<Value>,
}

/// Triggers a live test speech bubble on the overlay window.
#[tauri::command]
fn trigger_test_bubble(app: AppHandle, payload: Option<TestBubblePayload>) -> Result<(), String> {
    let p = payload.unwrap_or(TestBubblePayload {
        text: Some("말풍선 미리보기 테스트입니다! ✨".into()),
        settings: None,
    });
    let _ = app.emit("test-bubble", p);
    Ok(())
}

/// Gets the current AI agent connection status.
#[tauri::command]
fn get_agent_status() -> Result<session_watcher::AgentStatusInfo, String> {
    Ok(session_watcher::get_agent_status_snapshot())
}

/// Gets the list of user-defined custom watch paths from app_config.json.
#[tauri::command]
fn get_custom_watch_paths() -> Result<Vec<String>, String> {
    let src_dir = get_src_dir()?;
    let config = load_app_config(&src_dir);
    let paths = config
        .get("customWatchPaths")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    Ok(paths)
}

/// Adds a directory path to customWatchPaths in app_config.json.
#[tauri::command]
fn add_custom_watch_path(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("경로가 비어 있습니다.".into());
    }
    update_app_config(|config| {
    if let Some(obj) = config.as_object_mut() {
        let entry = obj
            .entry("customWatchPaths".to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Some(arr) = entry.as_array_mut() {
            let already = arr.iter().any(|v| v.as_str() == Some(trimmed));
            if !already {
                arr.push(Value::String(trimmed.to_string()));
            }
        }
    }
    })?;
    Ok(())
}

/// Removes a directory path from customWatchPaths in app_config.json.
#[tauri::command]
fn remove_custom_watch_path(path: String) -> Result<(), String> {
    update_app_config(|config| {
        if let Some(obj) = config.as_object_mut() {
            if let Some(arr) = obj.get_mut("customWatchPaths").and_then(Value::as_array_mut) {
                arr.retain(|v| v.as_str() != Some(&path));
            }
        }
    })?;
    Ok(())
}

/// Replays a sample agent session fixture for testing without needing the tool installed.
#[tauri::command]
fn replay_fixture_session(app: AppHandle, agent_id: String) -> Result<(), String> {
    let src_dir = get_src_dir()?;
    let project_root = src_dir.parent().unwrap_or(&src_dir);
    let fixture_path = project_root
        .join("tests")
        .join("fixtures")
        .join("agents")
        .join(&agent_id)
        .join("sample_session.jsonl");

    if !fixture_path.exists() {
        return Err(format!("픽스처 파일을 찾을 수 없습니다: {}", fixture_path.display()));
    }

    let content = fs::read_to_string(&fixture_path).map_err(|e| e.to_string())?;
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

    tauri::async_runtime::spawn(async move {
        let registry = session_watcher::AgentRegistry::new();
        for line in lines {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(val) = serde_json::from_str::<Value>(trimmed) {
                if let Some((state, detail, tool)) = registry.classify(Path::new("mock.jsonl"), &val) {
                    println!("[fixture_replay] Replaying {:?} ({:?}) for tool {}", state, detail, tool);
                    let _ = app.emit(
                        "state-change",
                        session_watcher::StateChangePayload { state, detail },
                    );
                }
            }
            tokio::time::sleep(Duration::from_millis(900)).await;
        }
    });

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
            pick_folder,
            parse_live2d_folder,
            save_live2d_character,
            delete_custom_character,
            save_bubble_settings,
            trigger_test_bubble,
            get_agent_status,
            get_custom_watch_paths,
            add_custom_watch_path,
            remove_custom_watch_path,
            replay_fixture_session,
            desktop::hide_to_tray,
            desktop::sync_tray_state,
            window_fit::report_character_zone
        ])
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "settings" => {
                api.prevent_close();
                let _ = window.hide();
            }
            tauri::WindowEvent::Moved(pos) if window.label() == "main" => {
                desktop::note_main_moved(pos.x, pos.y);
            }
            // Dragged onto a monitor with a different DPI scale: refit once things settle.
            tauri::WindowEvent::ScaleFactorChanged { .. } if window.label() == "main" => {
                if let Ok(pos) = window.outer_position() {
                    desktop::note_main_moved(pos.x, pos.y);
                }
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

            // Place the window (saved position or default), fit it to the monitor so the
            // character is always on-screen, then show it (main starts hidden to avoid a jump).
            if let Some(main_win) = app.get_webview_window("main") {
                let config = get_src_dir().map(|d| load_app_config(&d)).unwrap_or(Value::Null);
                desktop::place_main_at_startup(&main_win, &config);
                let _ = main_win.show();
            }
            tauri::async_runtime::spawn(desktop::run_position_saver(app.handle().clone()));

            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let get_custom_paths = || {
                    if let Ok(src_dir) = get_src_dir() {
                        let config = load_app_config(&src_dir);
                        if let Some(arr) = config.get("customWatchPaths").and_then(Value::as_array) {
                            return arr
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();
                        }
                    }
                    Vec::new()
                };
                session_watcher::run(app_handle, get_custom_paths).await;
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
