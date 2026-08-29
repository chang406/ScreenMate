//! Watches both Claude Code and Antigravity IDE/CLI session logs for
//! active conversation transcripts and classifies new lines as
//! idle/thinking/busy/talking/error/sleeping, emitting a `state-change` event.

use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const POLL_INTERVAL: Duration = Duration::from_millis(150);
const IDLE_TIMEOUT: Duration = Duration::from_secs(20);
const SLEEP_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Idle,
    Thinking,
    Busy,
    Talking,
    Error,
    Sleeping,
}

#[derive(Clone, Serialize)]
struct StateChangePayload {
    state: AgentState,
}

fn watch_directories() -> Vec<PathBuf> {
    let mut dirs_list = Vec::new();
    if let Some(home) = dirs::home_dir() {
        dirs_list.push(home.join(".claude").join("projects"));
        dirs_list.push(home.join(".gemini").join("antigravity-ide").join("brain"));
        dirs_list.push(home.join(".gemini").join("agy").join("logs"));
        dirs_list.push(home.join(".antigravity").join("logs"));
    }
    dirs_list
}

fn find_latest_session_file(roots: &[PathBuf]) -> Option<PathBuf> {
    fn walk(dir: &Path, best: &mut Option<(PathBuf, std::time::SystemTime)>) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, best);
                continue;
            }
            
            let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
            if !filename.ends_with(".jsonl") || filename == "transcript_full.jsonl" {
                continue;
            }

            let mtime = match entry.metadata().and_then(|m| m.modified()) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let better = match best {
                Some((_, best_mtime)) => mtime > *best_mtime,
                None => true,
            };
            if better {
                *best = Some((path, mtime));
            }
        }
    }

    let mut best = None;
    for root in roots {
        if root.exists() {
            walk(root, &mut best);
        }
    }
    best.map(|(path, _)| path)
}

fn classify_record(record: &Value) -> Option<AgentState> {
    // Check error status first
    if let Some(status) = record.get("status").and_then(Value::as_str) {
        if status.eq_ignore_ascii_case("ERROR") || status.eq_ignore_ascii_case("FAILED") {
            return Some(AgentState::Error);
        }
    }

    // 1. Antigravity format
    if let Some(record_type) = record.get("type").and_then(Value::as_str) {
        match record_type {
            "USER_INPUT" => return Some(AgentState::Thinking),
            "PLANNER_RESPONSE" => {
                let has_tool_calls = record
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .map(|arr| !arr.is_empty())
                    .unwrap_or(false);

                if has_tool_calls {
                    return Some(AgentState::Busy);
                }

                if let Some(content) = record.get("content").and_then(Value::as_str) {
                    if !content.trim().is_empty() {
                        return Some(AgentState::Talking);
                    }
                }

                if record.get("thinking").is_some() {
                    return Some(AgentState::Thinking);
                }
            }
            "TOOL_CALL" | "LIST_DIRECTORY" | "VIEW_FILE" | "RUN_COMMAND" | "GREP_SEARCH" => {
                return Some(AgentState::Busy);
            }
            // 2. Claude Code format
            "user" => return Some(AgentState::Thinking),
            "assistant" => {
                let content = record.get("message")?.get("content")?;

                if let Some(text) = content.as_str() {
                    return if text.trim().is_empty() {
                        None
                    } else {
                        Some(AgentState::Talking)
                    };
                }

                if let Some(blocks) = content.as_array() {
                    let has_tool_use = blocks
                        .iter()
                        .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"));
                    if has_tool_use {
                        return Some(AgentState::Busy);
                    }

                    let has_text = blocks.iter().any(|b| {
                        b.get("type").and_then(Value::as_str) == Some("text")
                            && b.get("text")
                                .and_then(Value::as_str)
                                .map(|t| !t.trim().is_empty())
                                .unwrap_or(false)
                    });
                    if has_text {
                        return Some(AgentState::Talking);
                    }
                }
                return None;
            }
            _ => {}
        }
    }

    if let Some(source) = record.get("source").and_then(Value::as_str) {
        match source {
            "USER_EXPLICIT" => return Some(AgentState::Thinking),
            "MODEL" => {
                if let Some(content) = record.get("content").and_then(Value::as_str) {
                    if !content.trim().is_empty() {
                        return Some(AgentState::Talking);
                    }
                }
                return Some(AgentState::Thinking);
            }
            _ => {}
        }
    }

    None
}

struct Tail {
    file: Option<PathBuf>,
    position: u64,
}

impl Tail {
    fn new() -> Self {
        Self {
            file: None,
            position: 0,
        }
    }

    fn switch_to(&mut self, path: PathBuf) {
        let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        self.position = len;
        self.file = Some(path);
    }

    fn read_new_records(&mut self) -> Vec<Value> {
        let Some(path) = self.file.clone() else {
            return Vec::new();
        };
        let mut file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.position {
            self.position = 0;
        }
        if file.seek(SeekFrom::Start(self.position)).is_err() {
            return Vec::new();
        }
        let mut buf = String::new();
        if file.read_to_string(&mut buf).is_err() {
            return Vec::new();
        }
        self.position += buf.len() as u64;

        buf.lines()
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() {
                    None
                } else {
                    serde_json::from_str::<Value>(line).ok()
                }
            })
            .collect()
    }
}

pub async fn run(app: AppHandle) {
    let watch_dirs = watch_directories();
    println!("[session_watcher] watching directories with multi-state support:");
    for d in &watch_dirs {
        println!("  - {}", d.display());
    }

    let mut tail = Tail::new();
    let mut current_state = AgentState::Idle;
    let mut last_activity: Option<Instant> = None;

    loop {
        if let Some(latest) = find_latest_session_file(&watch_dirs) {
            if tail.file.as_deref() != Some(latest.as_path()) {
                println!(
                    "[session_watcher] switching active session -> {}",
                    latest.display()
                );
                tail.switch_to(latest);
            }
        }

        for record in tail.read_new_records() {
            if let Some(new_state) = classify_record(&record) {
                last_activity = Some(Instant::now());
                if new_state != current_state {
                    current_state = new_state;
                    println!("[session_watcher] state -> {:?}", current_state);
                    let _ = app.emit(
                        "state-change",
                        StateChangePayload {
                            state: current_state,
                        },
                    );
                }
            }
        }

        if let Some(last) = last_activity {
            let elapsed = last.elapsed();
            if elapsed > SLEEP_TIMEOUT && current_state != AgentState::Sleeping {
                current_state = AgentState::Sleeping;
                println!("[session_watcher] sleep timeout -> Sleeping");
                let _ = app.emit(
                    "state-change",
                    StateChangePayload {
                        state: current_state,
                    },
                );
            } else if elapsed > IDLE_TIMEOUT && elapsed <= SLEEP_TIMEOUT && current_state != AgentState::Idle && current_state != AgentState::Sleeping {
                current_state = AgentState::Idle;
                println!("[session_watcher] idle timeout -> Idle");
                let _ = app.emit(
                    "state-change",
                    StateChangePayload {
                        state: current_state,
                    },
                );
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
