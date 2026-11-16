//! Locating the frontend `src/` directory and reading / writing `src/app_config.json`
//! (active character, global speech-bubble settings, custom watch paths).

use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Finds the `src` directory containing `characters.json`.
pub(crate) fn get_src_dir() -> Result<PathBuf, String> {
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
            return Ok(candidate
                .canonicalize()
                .unwrap_or_else(|_| candidate.clone()));
        }
    }

    // Default fallback
    if current.join("src").exists() {
        Ok(current.join("src"))
    } else {
        Ok(current.join("..").join("src"))
    }
}

/// Global speech-bubble defaults, used when `app_config.json` has none.
pub(crate) fn default_bubble_settings() -> Value {
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
}

pub(crate) fn get_app_config_path(src_dir: &Path) -> PathBuf {
    src_dir.join("app_config.json")
}

pub(crate) fn load_app_config(src_dir: &Path) -> Value {
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
        "globalBubbleSettings": default_bubble_settings()
    });
    let _ = fs::write(
        &path,
        serde_json::to_string_pretty(&default_config).unwrap_or_default(),
    );
    default_config
}

pub(crate) fn save_app_config(src_dir: &Path, config: &Value) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::scratch;

    #[test]
    fn load_app_config_creates_defaults_when_missing() {
        let dir = scratch("config");
        let cfg = load_app_config(&dir);
        assert_eq!(cfg["activeCharacter"], "mao");
        assert_eq!(cfg["customWatchPaths"], json!([]));
        assert_eq!(cfg["globalBubbleSettings"]["bgColor"], "#0f0f14");
        assert!(
            dir.join("app_config.json").exists(),
            "default file is written"
        );
        save_app_config(&dir, &json!({ "activeCharacter": "wanko" })).unwrap();
        assert_eq!(load_app_config(&dir)["activeCharacter"], "wanko");
    }

    #[test]
    fn default_bubble_settings_are_shared() {
        let dir = scratch("config_bubble");
        assert_eq!(
            load_app_config(&dir)["globalBubbleSettings"],
            default_bubble_settings()
        );
    }
}
