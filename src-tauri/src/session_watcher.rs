//! Watches AI agent session logs (Antigravity, Claude Code, ChatGPT/Codex, and custom paths)
//! for active conversation transcripts and classifies new lines into 7 core states:
//! idle, thinking, busy, talking, error, sleeping, emitting a `state-change` event.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

const POLL_INTERVAL: Duration = Duration::from_millis(150);
const IDLE_TIMEOUT: Duration = Duration::from_secs(20);
const SLEEP_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Idle,
    Thinking,
    Busy,
    Talking,
    Error,
    Sleeping,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct StateChangePayload {
    pub state: AgentState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WatchedPathInfo {
    pub path: String,
    pub source: String, // "default" | "custom" | "env"
    pub exists: bool,
    pub session_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentStatusInfo {
    pub is_connected: bool,
    pub active_tool: Option<String>,
    pub active_session_file: Option<String>,
    pub last_event_time: Option<u64>,
    pub watched_paths: Vec<WatchedPathInfo>,
}

impl Default for AgentStatusInfo {
    fn default() -> Self {
        Self {
            is_connected: false,
            active_tool: None,
            active_session_file: None,
            last_event_time: None,
            watched_paths: Vec::new(),
        }
    }
}

static GLOBAL_AGENT_STATUS: OnceLock<Arc<Mutex<AgentStatusInfo>>> = OnceLock::new();

fn get_status_lock() -> &'static Arc<Mutex<AgentStatusInfo>> {
    GLOBAL_AGENT_STATUS.get_or_init(|| Arc::new(Mutex::new(AgentStatusInfo::default())))
}

pub fn get_agent_status_snapshot() -> AgentStatusInfo {
    get_status_lock().lock().unwrap().clone()
}

pub fn update_agent_status<F>(updater: F)
where
    F: FnOnce(&mut AgentStatusInfo),
{
    let lock = get_status_lock();
    if let Ok(mut status) = lock.lock() {
        updater(&mut status);
    }
}

// ---------------------------------------------------------------------------
// Agent Log Adapter Trait
// ---------------------------------------------------------------------------

pub trait AgentLogAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn default_watch_dirs(&self) -> Vec<PathBuf>;
    fn is_session_file(&self, path: &Path) -> bool;
    fn classify_record(&self, record: &Value) -> Option<(AgentState, Option<String>)>;
}

fn tool_category(tool_name: &str) -> &'static str {
    let lower = tool_name.to_lowercase();
    if lower.contains("replace") || lower.contains("write") || lower.contains("edit") {
        "edit"
    } else if lower.contains("command")
        || lower.contains("bash")
        || lower.contains("terminal")
        || lower.contains("run")
    {
        "command"
    } else if lower.contains("view")
        || lower.contains("list")
        || lower.contains("read")
        || lower.contains("grep")
        || lower.contains("search")
        || lower.contains("find")
    {
        "read"
    } else if lower.contains("browser") || lower.contains("url") || lower.contains("web") {
        "browse"
    } else {
        "general"
    }
}

// ---------------------------------------------------------------------------
// 1. Antigravity Adapter (Google Antigravity IDE / CLI)
// ---------------------------------------------------------------------------

pub struct AntigravityAdapter;

impl AgentLogAdapter for AntigravityAdapter {
    fn id(&self) -> &'static str {
        "antigravity"
    }

    fn display_name(&self) -> &'static str {
        "Antigravity (IDE / CLI)"
    }

    fn default_watch_dirs(&self) -> Vec<PathBuf> {
        let mut list = Vec::new();
        if let Some(home) = dirs::home_dir() {
            list.push(home.join(".gemini").join("antigravity-ide").join("brain"));
            list.push(home.join(".gemini").join("agy").join("logs"));
            list.push(home.join(".antigravity").join("logs"));
        }
        list
    }

    fn is_session_file(&self, path: &Path) -> bool {
        let name = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        // Must be .jsonl, not transcript_full.jsonl to avoid huge file offsets
        (name == "transcript.jsonl" || name.ends_with(".jsonl")) && name != "transcript_full.jsonl"
    }

    fn classify_record(&self, record: &Value) -> Option<(AgentState, Option<String>)> {
        // Status error check
        if let Some(status) = record.get("status").and_then(Value::as_str) {
            if status.eq_ignore_ascii_case("ERROR") || status.eq_ignore_ascii_case("FAILED") {
                return Some((AgentState::Error, None));
            }
        }

        if let Some(record_type) = record.get("type").and_then(Value::as_str) {
            match record_type {
                "USER_INPUT" => return Some((AgentState::Thinking, None)),
                "PLANNER_RESPONSE" => {
                    if let Some(arr) = record.get("tool_calls").and_then(Value::as_array) {
                        if !arr.is_empty() {
                            let detail = arr
                                .first()
                                .and_then(|t| t.get("name").and_then(Value::as_str))
                                .map(tool_category)
                                .unwrap_or("general");
                            return Some((AgentState::Busy, Some(detail.to_string())));
                        }
                    }
                    if let Some(content) = record.get("content").and_then(Value::as_str) {
                        if !content.trim().is_empty() {
                            return Some((AgentState::Talking, None));
                        }
                    }
                    return None;
                }
                "REPLACE_FILE_CONTENT" | "MULTI_REPLACE_FILE_CONTENT" | "WRITE_TO_FILE" => {
                    return Some((AgentState::Busy, Some("edit".to_string())));
                }
                "RUN_COMMAND" => {
                    return Some((AgentState::Busy, Some("command".to_string())));
                }
                "VIEW_FILE" | "LIST_DIRECTORY" | "GREP_SEARCH" | "SEARCH_WEB" => {
                    return Some((AgentState::Busy, Some("read".to_string())));
                }
                "READ_URL_CONTENT" | "BROWSER_SUBAGENT" => {
                    return Some((AgentState::Busy, Some("browse".to_string())));
                }
                "TOOL_CALL" | "TOOL_RESULT" | "progress" | "GENERATE_IMAGE" => {
                    return Some((AgentState::Busy, Some("general".to_string())));
                }
                _ => {}
            }
        }

        if let Some(source) = record.get("source").and_then(Value::as_str) {
            if source == "USER_EXPLICIT" {
                return Some((AgentState::Thinking, None));
            } else if source == "MODEL" {
                return Some((AgentState::Busy, Some("general".to_string())));
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// 2. Claude Adapter (Anthropic Claude Code)
// ---------------------------------------------------------------------------

pub struct ClaudeAdapter;

impl AgentLogAdapter for ClaudeAdapter {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
    }

    fn default_watch_dirs(&self) -> Vec<PathBuf> {
        let mut list = Vec::new();
        if let Some(home) = dirs::home_dir() {
            list.push(home.join(".claude").join("projects"));
        }
        list
    }

    fn is_session_file(&self, path: &Path) -> bool {
        let name = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        name.ends_with(".jsonl") && name != "transcript_full.jsonl"
    }

    fn classify_record(&self, record: &Value) -> Option<(AgentState, Option<String>)> {
        if let Some(status) = record.get("status").and_then(Value::as_str) {
            if status.eq_ignore_ascii_case("error") || status.eq_ignore_ascii_case("failed") {
                return Some((AgentState::Error, None));
            }
        }

        if let Some(record_type) = record.get("type").and_then(Value::as_str) {
            match record_type {
                "user" => return Some((AgentState::Thinking, None)),
                "assistant" => {
                    let message = record.get("message");
                    let content = message
                        .and_then(|m| m.get("content"))
                        .or_else(|| record.get("content"));

                    if let Some(content) = content {
                        if let Some(blocks) = content.as_array() {
                            if let Some(tb) = blocks.iter().find(|b| {
                                b.get("type").and_then(Value::as_str) == Some("tool_use")
                            }) {
                                let tool_name = tb.get("name").and_then(Value::as_str).unwrap_or("");
                                return Some((AgentState::Busy, Some(tool_category(tool_name).to_string())));
                            }

                            if blocks.iter().any(|b| {
                                b.get("type").and_then(Value::as_str) == Some("thinking")
                            }) {
                                return Some((AgentState::Thinking, None));
                            }

                            if blocks.iter().any(|b| {
                                b.get("type").and_then(Value::as_str) == Some("text")
                                    && b.get("text")
                                        .and_then(Value::as_str)
                                        .map(|t| !t.trim().is_empty())
                                        .unwrap_or(false)
                            }) {
                                return Some((AgentState::Talking, None));
                            }
                        } else if let Some(text) = content.as_str() {
                            if !text.trim().is_empty() {
                                return Some((AgentState::Talking, None));
                            }
                        }
                    }

                    let stop_reason = message
                        .and_then(|m| m.get("stop_reason"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    if stop_reason == "tool_use" {
                        return Some((AgentState::Busy, Some("general".to_string())));
                    }
                }
                "tool_result" => return Some((AgentState::Busy, Some("general".to_string()))),
                _ => {}
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// 3. ChatGPT / OpenAI Codex Adapter
// ---------------------------------------------------------------------------

pub struct ChatGptCodexAdapter;

impl AgentLogAdapter for ChatGptCodexAdapter {
    fn id(&self) -> &'static str {
        "chatgpt_codex"
    }

    fn display_name(&self) -> &'static str {
        "ChatGPT / OpenAI Codex"
    }

    fn default_watch_dirs(&self) -> Vec<PathBuf> {
        let mut list = Vec::new();
        if let Some(home) = dirs::home_dir() {
            list.push(home.join(".codex").join("sessions"));
            list.push(home.join(".openai").join("sessions"));
            list.push(home.join(".chatgpt").join("sessions"));
        }
        list
    }

    fn is_session_file(&self, path: &Path) -> bool {
        let name = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
        name.ends_with(".jsonl") || name.ends_with(".json")
    }

    fn classify_record(&self, record: &Value) -> Option<(AgentState, Option<String>)> {
        // Error flag
        if let Some(err) = record.get("error").or_else(|| record.get("status")) {
            if err.as_str().map(|s| s.eq_ignore_ascii_case("failed") || s.eq_ignore_ascii_case("error")).unwrap_or(false) || err.is_object() {
                return Some((AgentState::Error, None));
            }
        }

        // Standard OpenAI Chat role structure
        if let Some(role) = record.get("role").and_then(Value::as_str) {
            match role {
                "user" => return Some((AgentState::Thinking, None)),
                "assistant" => {
                    if record.get("function_call").is_some() || record.get("tool_calls").is_some() {
                        let name = record
                            .get("function_call")
                            .and_then(|f| f.get("name").and_then(Value::as_str))
                            .unwrap_or("general");
                        return Some((AgentState::Busy, Some(tool_category(name).to_string())));
                    }
                    if let Some(content) = record.get("content").and_then(Value::as_str) {
                        if !content.trim().is_empty() {
                            return Some((AgentState::Talking, None));
                        }
                    }
                }
                "function" | "tool" => {
                    return Some((AgentState::Busy, Some("general".to_string())));
                }
                _ => {}
            }
        }

        None
    }
}

// ---------------------------------------------------------------------------
// Agent Registry
// ---------------------------------------------------------------------------

pub struct AgentRegistry {
    adapters: Vec<Box<dyn AgentLogAdapter>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            adapters: vec![
                Box::new(AntigravityAdapter),
                Box::new(ClaudeAdapter),
                Box::new(ChatGptCodexAdapter),
            ],
        }
    }

    pub fn adapters(&self) -> &[Box<dyn AgentLogAdapter>] {
        &self.adapters
    }

    pub fn match_adapter_for_path<'a>(&'a self, path: &Path) -> Option<&'a Box<dyn AgentLogAdapter>> {
        let path_str = path.to_string_lossy().to_lowercase();
        // Priority check by path keywords
        if path_str.contains("gemini") || path_str.contains("antigravity") {
            return self.adapters.iter().find(|a| a.id() == "antigravity");
        }
        if path_str.contains("claude") {
            return self.adapters.iter().find(|a| a.id() == "claude");
        }
        if path_str.contains("codex") || path_str.contains("openai") || path_str.contains("chatgpt") {
            return self.adapters.iter().find(|a| a.id() == "chatgpt_codex");
        }
        // Fallback: first adapter that accepts the file format
        self.adapters.iter().find(|a| a.is_session_file(path))
    }

    pub fn classify(&self, path: &Path, record: &Value) -> Option<(AgentState, Option<String>, &'static str)> {
        // Try matched adapter first
        if let Some(matched) = self.match_adapter_for_path(path) {
            if let Some(res) = matched.classify_record(record) {
                return Some((res.0, res.1, matched.id()));
            }
        }
        // Try other adapters as fallback
        for adapter in &self.adapters {
            if let Some(res) = adapter.classify_record(record) {
                return Some((res.0, res.1, adapter.id()));
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Watcher Engine & Tail
// ---------------------------------------------------------------------------

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

/// Collects all directories to watch by merging adapter defaults, environment variables, and user custom paths.
pub fn resolve_all_watch_directories(
    registry: &AgentRegistry,
    custom_paths: &[String],
) -> Vec<WatchedPathInfo> {
    let mut results = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 1. Custom Paths from app_config.json
    for cp in custom_paths {
        let p = PathBuf::from(cp);
        let exists = p.exists();
        let session_count = if exists { count_session_files(&p) } else { 0 };
        let key = p.to_string_lossy().to_string();
        if seen.insert(key.clone()) {
            results.push(WatchedPathInfo {
                path: key,
                source: "custom".to_string(),
                exists,
                session_count,
            });
        }
    }

    // 2. System Environment Variable: SCREENMATE_WATCH_DIRS
    if let Ok(env_dirs) = std::env::var("SCREENMATE_WATCH_DIRS") {
        for part in env_dirs.split(';') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                let p = PathBuf::from(trimmed);
                let exists = p.exists();
                let session_count = if exists { count_session_files(&p) } else { 0 };
                let key = p.to_string_lossy().to_string();
                if seen.insert(key.clone()) {
                    results.push(WatchedPathInfo {
                        path: key,
                        source: "env".to_string(),
                        exists,
                        session_count,
                    });
                }
            }
        }
    }

    // 3. Defaults from all adapters
    for adapter in registry.adapters() {
        for p in adapter.default_watch_dirs() {
            let exists = p.exists();
            let session_count = if exists { count_session_files(&p) } else { 0 };
            let key = p.to_string_lossy().to_string();
            if seen.insert(key.clone()) {
                results.push(WatchedPathInfo {
                    path: key,
                    source: "default".to_string(),
                    exists,
                    session_count,
                });
            }
        }
    }

    results
}

fn count_session_files(dir: &Path) -> usize {
    let mut count = 0;
    fn walk_count(d: &Path, count: &mut usize, depth: usize) {
        if depth > 4 {
            return;
        }
        if let Ok(entries) = fs::read_dir(d) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk_count(&path, count, depth + 1);
                } else if let Some(name) = path.file_name().and_then(|f| f.to_str()) {
                    if name.ends_with(".jsonl") && name != "transcript_full.jsonl" {
                        *count += 1;
                    }
                }
            }
        }
    }
    walk_count(dir, &mut count, 0);
    count
}

fn find_latest_session_file(paths: &[WatchedPathInfo]) -> Option<PathBuf> {
    fn walk(dir: &Path, best: &mut Option<(PathBuf, SystemTime)>, depth: usize) {
        if depth > 5 {
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, best, depth + 1);
                continue;
            }

            let filename = path.file_name().and_then(|f| f.to_str()).unwrap_or("");
            if !filename.ends_with(".jsonl") && !filename.ends_with(".json") {
                continue;
            }
            if filename == "transcript_full.jsonl" || filename == "character.json" || filename == "app_config.json" {
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
    for info in paths {
        if info.exists {
            walk(Path::new(&info.path), &mut best, 0);
        }
    }
    best.map(|(path, _)| path)
}

pub async fn run(app: AppHandle, get_custom_paths_fn: impl Fn() -> Vec<String> + Send + 'static) {
    let registry = AgentRegistry::new();
    let mut tail = Tail::new();
    let mut current_state = AgentState::Idle;
    let mut last_activity: Option<Instant> = None;
    let mut check_dirs_counter = 0usize;
    let mut cached_watch_info: Vec<WatchedPathInfo> = Vec::new();

    println!("[session_watcher] Multi-agent registry initialized with adapters:");
    for a in registry.adapters() {
        println!("  ✓ [{}] {}", a.id(), a.display_name());
    }

    loop {
        // Refresh watch directories periodically (every ~3 seconds = 20 loops)
        if check_dirs_counter % 20 == 0 {
            let custom_paths = get_custom_paths_fn();
            cached_watch_info = resolve_all_watch_directories(&registry, &custom_paths);
            update_agent_status(|status| {
                status.watched_paths = cached_watch_info.clone();
            });
        }
        check_dirs_counter = check_dirs_counter.wrapping_add(1);

        // Check for latest session file
        if let Some(latest) = find_latest_session_file(&cached_watch_info) {
            if tail.file.as_deref() != Some(latest.as_path()) {
                println!(
                    "[session_watcher] Active session detected: {}",
                    latest.display()
                );
                tail.switch_to(latest.clone());

                let active_tool = registry
                    .match_adapter_for_path(&latest)
                    .map(|a| a.id().to_string())
                    .unwrap_or_else(|| "custom".to_string());

                update_agent_status(|status| {
                    status.is_connected = true;
                    status.active_tool = Some(active_tool);
                    status.active_session_file = Some(latest.to_string_lossy().to_string());
                });

                let _ = app.emit("agent-status-changed", get_agent_status_snapshot());
            }
        }

        // Read delta records
        for record in tail.read_new_records() {
            let current_path = tail.file.as_deref().unwrap_or(Path::new(""));
            if let Some((new_state, detail, tool_id)) = registry.classify(current_path, &record) {
                last_activity = Some(Instant::now());

                let now_epoch = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                update_agent_status(|status| {
                    status.is_connected = true;
                    status.active_tool = Some(tool_id.to_string());
                    status.last_event_time = Some(now_epoch);
                });

                if new_state != current_state || (new_state == AgentState::Busy && detail.is_some()) {
                    current_state = new_state;
                    println!(
                        "[session_watcher] [tool:{}] state -> {:?}, detail -> {:?}",
                        tool_id, current_state, detail
                    );
                    let _ = app.emit(
                        "state-change",
                        StateChangePayload {
                            state: current_state,
                            detail,
                        },
                    );
                    let _ = app.emit("agent-status-changed", get_agent_status_snapshot());
                }
            }
        }

        // Idle / Sleep timeouts
        if let Some(last) = last_activity {
            let elapsed = last.elapsed();
            if elapsed > SLEEP_TIMEOUT && current_state != AgentState::Sleeping {
                current_state = AgentState::Sleeping;
                println!("[session_watcher] sleep timeout -> Sleeping");
                let _ = app.emit(
                    "state-change",
                    StateChangePayload {
                        state: current_state,
                        detail: None,
                    },
                );
            } else if elapsed > IDLE_TIMEOUT
                && elapsed <= SLEEP_TIMEOUT
                && current_state != AgentState::Idle
                && current_state != AgentState::Sleeping
            {
                current_state = AgentState::Idle;
                println!("[session_watcher] idle timeout -> Idle");
                let _ = app.emit(
                    "state-change",
                    StateChangePayload {
                        state: current_state,
                        detail: None,
                    },
                );
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(test)]
#[path = "session_watcher_tests.rs"]
mod tests;
