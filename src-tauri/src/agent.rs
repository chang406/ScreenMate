//! AI agent connection commands for the character manager's "AI 에이전트 연결" tab:
//! connection status, custom watch paths and fixture replay (simulating an agent session).

use crate::characters::validate_segment;
use crate::config::{get_src_dir, load_app_config, update_app_config};
use crate::session_watcher;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Gets the current AI agent connection status.
#[tauri::command]
pub fn get_agent_status() -> Result<session_watcher::AgentStatusInfo, String> {
    Ok(session_watcher::get_agent_status_snapshot())
}

/// Gets the list of user-defined custom watch paths from app_config.json.
#[tauri::command]
pub fn get_custom_watch_paths() -> Result<Vec<String>, String> {
    get_src_dir()?;
    Ok(custom_watch_paths())
}

/// `customWatchPaths` from `app_config.json` (empty if the file or key is missing).
/// Also polled by the session watcher every few seconds.
pub(crate) fn custom_watch_paths() -> Vec<String> {
    let Ok(src_dir) = get_src_dir() else {
        return Vec::new();
    };
    load_app_config(&src_dir)
        .get("customWatchPaths")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Adds a directory path to customWatchPaths in app_config.json.
#[tauri::command]
pub fn add_custom_watch_path(path: String) -> Result<(), String> {
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
pub fn remove_custom_watch_path(path: String) -> Result<(), String> {
    update_app_config(|config| {
        if let Some(obj) = config.as_object_mut() {
            if let Some(arr) = obj
                .get_mut("customWatchPaths")
                .and_then(Value::as_array_mut)
            {
                arr.retain(|v| v.as_str() != Some(&path));
            }
        }
    })?;
    Ok(())
}

/// Replays a sample agent session fixture for testing without needing the tool installed.
#[tauri::command]
pub fn replay_fixture_session(app: AppHandle, agent_id: String) -> Result<(), String> {
    validate_segment(&agent_id, "에이전트 ID")?;
    let src_dir = get_src_dir()?;
    let project_root = src_dir.parent().unwrap_or(&src_dir);
    let fixture_path = project_root
        .join("tests")
        .join("fixtures")
        .join("agents")
        .join(&agent_id)
        .join("sample_session.jsonl");

    if !fixture_path.exists() {
        return Err(format!(
            "픽스처 파일을 찾을 수 없습니다: {}",
            fixture_path.display()
        ));
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
                if let Some((state, detail, tool)) =
                    registry.classify(Path::new("mock.jsonl"), &val)
                {
                    println!(
                        "[fixture_replay] Replaying {:?} ({:?}) for tool {}",
                        state, detail, tool
                    );
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
