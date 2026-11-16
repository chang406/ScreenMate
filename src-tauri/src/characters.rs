//! Character registry commands: listing characters (folder scan), switching the active one,
//! saving 2D image characters, deleting custom characters and speech-bubble settings.
//!
//! Every ID coming from the webview is validated with [`validate_character_id`] before it is
//! joined onto a real directory.

use crate::config::{default_bubble_settings, get_src_dir, load_app_config, update_app_config};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

/// Validates a name that becomes a single path segment (character ID, state name, agent ID).
///
/// These values come from the webview and are joined onto real directories that are later
/// written to or deleted with `remove_dir_all`. Without this check an ID such as `..` would
/// resolve to the parent folder (e.g. `custom_characters/..` = every character). The settings UI
/// already restricts input, but the backend is the last line of defence.
pub(crate) fn validate_segment(value: &str, what: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{what} '{value}'은(는) 사용할 수 없습니다. 영문, 숫자, '_', '-'만 1~64자로 입력해 주세요."
        ))
    }
}

/// Validates a character ID (folder name under `characters/` or `custom_characters/`).
pub(crate) fn validate_character_id(id: &str) -> Result<(), String> {
    validate_segment(id, "캐릭터 ID")
}

/// Finds the `character.json` file for a given character ID.
/// Checks custom_characters first, then standard characters.
pub(crate) fn find_character_json_path(src_dir: &Path, char_id: &str) -> Option<PathBuf> {
    let custom_path = src_dir
        .join("characters")
        .join("custom_characters")
        .join(char_id)
        .join("character.json");
    if custom_path.exists() {
        return Some(custom_path);
    }
    let default_path = src_dir
        .join("characters")
        .join(char_id)
        .join("character.json");
    if default_path.exists() {
        return Some(default_path);
    }
    None
}

/// Adds `<dir>/<id>/character.json` of every sub-folder of `dir` (except `skip`) to `out`,
/// keyed by folder name. Unreadable or invalid JSON files are skipped.
fn scan_character_folders(dir: &Path, skip: &str, out: &mut serde_json::Map<String, Value>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if !path.is_dir() || name == skip {
            continue;
        }
        let parsed = fs::read_to_string(path.join("character.json"))
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok());
        if let Some(char_val) = parsed {
            out.insert(name, char_val);
        }
    }
}

/// Scans character directories and merges them with app_config.json.
/// Note: The root `characters.json` is treated as a sample/template reference only and ignored here.
#[tauri::command]
pub fn get_character_configs() -> Result<Value, String> {
    let src_dir = get_src_dir()?;
    let mut app_config = load_app_config(&src_dir);

    let mut characters = serde_json::Map::new();

    // Public characters in characters/<id>/, then private ones in custom_characters/<id>/
    // (a custom character with the same ID overrides the public one).
    let chars_dir = src_dir.join("characters");
    scan_character_folders(&chars_dir, "custom_characters", &mut characters);
    scan_character_folders(
        &chars_dir.join("custom_characters"),
        "_template",
        &mut characters,
    );

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
        .unwrap_or_else(default_bubble_settings);

    Ok(json!({
        "activeCharacter": active,
        "characters": characters,
        "globalBubbleSettings": global_bubble
    }))
}

/// Sets the active character and notifies the main window.
#[tauri::command]
pub fn set_active_character(app: AppHandle, id: String) -> Result<(), String> {
    validate_character_id(&id)?;
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
pub fn save_2d_character(app: AppHandle, payload: Save2DCharacterPayload) -> Result<(), String> {
    validate_character_id(&payload.id)?;
    for state in payload.files.keys() {
        // State names become file names (`<state>.<ext>`) inside the character folder.
        validate_segment(state, "상태 이름")?;
    }
    let src_dir = get_src_dir()?;
    let custom_char_dir = src_dir
        .join("characters")
        .join("custom_characters")
        .join(&payload.id);
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

            let bytes = BASE64_STANDARD
                .decode(base64_data)
                .map_err(|e| format!("Base64 decode error: {}", e))?;
            (extension, bytes)
        } else {
            // Raw text or fallback
            ("png", data_url.as_bytes().to_vec())
        };

        let file_name = format!("{}.{}", state, ext);
        let file_path = custom_char_dir.join(&file_name);
        fs::write(&file_path, raw_bytes).map_err(|e| e.to_string())?;

        let relative_src = format!("characters/custom_characters/{}/{}", payload.id, file_name);
        let subs = payload
            .subtitles
            .get(&state)
            .cloned()
            .unwrap_or_else(|| match state.as_str() {
                "idle" => vec!["대기 중이에요~".to_string()],
                "thinking" => vec!["생각 중이에요...".to_string()],
                "talking" => vec!["작업 완료! ✨".to_string()],
                _ => vec!["대기 중".to_string()],
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

/// Deletes a custom character directory.
#[tauri::command]
pub fn delete_custom_character(app: AppHandle, id: String) -> Result<(), String> {
    validate_character_id(&id)?;
    if id == "mao" || id == "pixel_cat" || id == "wanko" {
        return Err("기본 프리셋 캐릭터는 삭제할 수 없습니다.".into());
    }

    let src_dir = get_src_dir()?;
    let custom_char_dir = src_dir
        .join("characters")
        .join("custom_characters")
        .join(&id);

    if custom_char_dir.exists() {
        let _ = fs::remove_dir_all(&custom_char_dir);
    }

    let mut need_switch = false;
    let _ = update_app_config(|cfg| {
        if cfg.get("activeCharacter").and_then(|a| a.as_str()) == Some(id.as_str()) {
            need_switch = true;
            if let Some(obj) = cfg.as_object_mut() {
                obj.insert(
                    "activeCharacter".to_string(),
                    Value::String("mao".to_string()),
                );
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
pub fn save_bubble_settings(
    app: AppHandle,
    payload: SaveBubbleSettingsPayload,
) -> Result<(), String> {
    let src_dir = get_src_dir()?;

    if let Some(target) = payload.target_id.as_deref() {
        if target != "global" {
            validate_character_id(target)?;
        }
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
            return Err(format!(
                "캐릭터 '{}'의 설정 파일을 찾을 수 없습니다.",
                target
            ));
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
pub fn trigger_test_bubble(
    app: AppHandle,
    payload: Option<TestBubblePayload>,
) -> Result<(), String> {
    let p = payload.unwrap_or(TestBubblePayload {
        text: Some("말풍선 미리보기 테스트입니다! ✨".into()),
        settings: None,
    });
    let _ = app.emit("test-bubble", p);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{scratch, write};

    #[test]
    fn valid_character_ids_are_accepted() {
        for id in ["mao", "pixel_cat", "my-char_2", "A", &"x".repeat(64)] {
            assert!(validate_character_id(id).is_ok(), "{id}");
        }
    }

    #[test]
    fn path_like_or_odd_ids_are_rejected() {
        // `..` would turn custom_characters/<id> into the parent folder (all characters).
        for id in [
            "",
            "..",
            ".",
            "../mao",
            "..\\mao",
            "a/b",
            "a\\b",
            "C:",
            "my char",
            "캐릭터",
            "x\0y",
            &"x".repeat(65),
        ] {
            assert!(
                validate_character_id(id).is_err(),
                "{id:?} must be rejected"
            );
        }
    }

    #[test]
    fn error_message_names_the_field() {
        let err = validate_segment("..", "상태 이름").unwrap_err();
        assert!(err.contains("상태 이름") && err.contains(".."));
    }

    #[test]
    fn scan_reads_character_folders_and_skips_excluded_and_invalid() {
        let dir = scratch("scan_chars");
        write(&dir.join("cat/character.json"), r#"{"name":"Cat"}"#);
        write(&dir.join("broken/character.json"), "{ not json");
        write(&dir.join("empty/readme.txt"), "no character.json");
        write(
            &dir.join("custom_characters/mine/character.json"),
            r#"{"name":"Mine"}"#,
        );
        write(&dir.join("loose.json"), "{}");
        let mut out = serde_json::Map::new();
        scan_character_folders(&dir, "custom_characters", &mut out);
        let mut keys: Vec<_> = out.keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, vec!["cat"]);
        // custom folder scanned separately; same id overrides the public one
        write(
            &dir.join("custom_characters/cat/character.json"),
            r#"{"name":"My Cat"}"#,
        );
        scan_character_folders(&dir.join("custom_characters"), "_template", &mut out);
        assert_eq!(out["cat"]["name"], "My Cat");
        assert_eq!(out["mine"]["name"], "Mine");
        // missing folder is not an error
        scan_character_folders(&dir.join("nope"), "", &mut out);
    }
}
