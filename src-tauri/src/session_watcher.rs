//! Watches AI agent session logs (Antigravity, Claude Code, ChatGPT/Codex, and custom paths)
//! for active conversation transcripts and classifies new lines into 7 core states:
//! idle, thinking, busy, talking, error, sleeping, emitting a `state-change` event.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// How often the active session file is tailed (reaction latency).
const POLL_INTERVAL: Duration = Duration::from_millis(150);
/// How often watch roots are re-walked to discover a newer session file.
/// Walking thousands of files every 150ms was the single biggest idle cost of the app.
const RESCAN_INTERVAL: Duration = Duration::from_millis(1000);
/// How often `app_config.json` custom paths are re-read.
const CONFIG_REFRESH_INTERVAL: Duration = Duration::from_secs(3);
const IDLE_TIMEOUT: Duration = Duration::from_secs(20);
const SLEEP_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes
/// A newly discovered session file smaller than this is read from the beginning so the
/// very first prompt of a brand-new conversation is not missed. Larger files (e.g. a
/// resumed conversation with long history) start from the end to avoid replaying history.
const NEW_SESSION_REPLAY_LIMIT: u64 = 64 * 1024;
/// Maximum directory depth walked below each watch root.
const MAX_SCAN_DEPTH: usize = 5;

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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AgentStatusInfo {
    pub is_connected: bool,
    pub active_tool: Option<String>,
    pub active_session_file: Option<String>,
    pub last_event_time: Option<u64>,
    pub watched_paths: Vec<WatchedPathInfo>,
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
    if lower.contains("replace")
        || lower.contains("write")
        || lower.contains("edit")
        || lower.contains("patch")
    {
        "edit"
    } else if lower.contains("command")
        || lower.contains("bash")
        || lower.contains("terminal")
        || lower.contains("shell")
        || lower.contains("exec")
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

        // API failures (overload, rate limit, auth) are written as assistant messages with this flag.
        if record.get("isApiErrorMessage").and_then(Value::as_bool) == Some(true) {
            return Some((AgentState::Error, None));
        }

        // Meta records (slash-command caveats, local command output) are not agent activity.
        if record.get("isMeta").and_then(Value::as_bool) == Some(true) {
            return None;
        }

        if let Some(record_type) = record.get("type").and_then(Value::as_str) {
            match record_type {
                "user" => {
                    let content = record
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .or_else(|| record.get("content"));
                    if let Some(blocks) = content.and_then(Value::as_array) {
                        // Claude Code records tool results as user messages: the agent is still working.
                        if blocks
                            .iter()
                            .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
                        {
                            return Some((AgentState::Busy, Some("general".to_string())));
                        }
                        let interrupted = blocks.iter().any(|b| {
                            b.get("text")
                                .and_then(Value::as_str)
                                .map(|t| t.starts_with("[Request interrupted"))
                                .unwrap_or(false)
                        });
                        if interrupted {
                            return Some((AgentState::Idle, None));
                        }
                    } else if let Some(text) = content.and_then(Value::as_str) {
                        if text.starts_with("[Request interrupted") {
                            return Some((AgentState::Idle, None));
                        }
                    }
                    return Some((AgentState::Thinking, None));
                }
                "system" => {
                    let level = record.get("level").and_then(Value::as_str).unwrap_or("");
                    if level.eq_ignore_ascii_case("error") {
                        return Some((AgentState::Error, None));
                    }
                    return None;
                }
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
        // Current Codex CLI "rollout" format: every line is a typed envelope with a payload.
        match record.get("type").and_then(Value::as_str) {
            Some("response_item") => return classify_codex_response_item(record.get("payload")?),
            Some("event_msg") => return classify_codex_event(record.get("payload")?),
            Some("session_meta") | Some("turn_context") | Some("compacted") => return None,
            _ => {}
        }

        // Legacy flat format below.
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

/// Codex rollout `response_item` payloads (model-facing conversation items).
fn classify_codex_response_item(payload: &Value) -> Option<(AgentState, Option<String>)> {
    match payload.get("type").and_then(Value::as_str)? {
        "message" => {
            let text_of = |b: &Value| b.get("text").and_then(Value::as_str).unwrap_or("").trim().to_string();
            let texts: Vec<String> = payload
                .get("content")
                .and_then(Value::as_array)
                .map(|arr| arr.iter().map(text_of).filter(|t| !t.is_empty()).collect())
                .unwrap_or_default();
            match payload.get("role").and_then(Value::as_str)? {
                "user" => {
                    // Codex injects environment/instruction blocks as user messages at session start.
                    let injected = !texts.is_empty()
                        && texts.iter().all(|t| {
                            t.starts_with("<environment_context>") || t.starts_with("<user_instructions>")
                        });
                    if injected {
                        None
                    } else {
                        Some((AgentState::Thinking, None))
                    }
                }
                "assistant" if !texts.is_empty() => Some((AgentState::Talking, None)),
                _ => None,
            }
        }
        "reasoning" => Some((AgentState::Thinking, None)),
        "function_call" | "custom_tool_call" => {
            let name = payload.get("name").and_then(Value::as_str).unwrap_or("general");
            Some((AgentState::Busy, Some(tool_category(name).to_string())))
        }
        "local_shell_call" => Some((AgentState::Busy, Some("command".to_string()))),
        "web_search_call" => Some((AgentState::Busy, Some("browse".to_string()))),
        "function_call_output" | "custom_tool_call_output" => {
            Some((AgentState::Busy, Some("general".to_string())))
        }
        _ => None,
    }
}

/// Codex rollout `event_msg` payloads (UI-facing events). Only the ones that carry
/// state information not already conveyed by `response_item`s are mapped.
fn classify_codex_event(payload: &Value) -> Option<(AgentState, Option<String>)> {
    match payload.get("type").and_then(Value::as_str)? {
        "user_message" => Some((AgentState::Thinking, None)),
        "agent_reasoning" => Some((AgentState::Thinking, None)),
        "agent_message" => Some((AgentState::Talking, None)),
        "exec_command_begin" => Some((AgentState::Busy, Some("command".to_string()))),
        "patch_apply_begin" => Some((AgentState::Busy, Some("edit".to_string()))),
        "error" | "stream_error" => Some((AgentState::Error, None)),
        "turn_aborted" => Some((AgentState::Idle, None)),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Agent Registry
// ---------------------------------------------------------------------------

pub struct AgentRegistry {
    adapters: Vec<Box<dyn AgentLogAdapter>>,
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
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

    fn by_id(&self, id: &str) -> Option<&dyn AgentLogAdapter> {
        self.adapters.iter().find(|a| a.id() == id).map(|a| a.as_ref())
    }

    /// Identifies the tool that owns `path` from its location, if possible.
    ///
    /// Tool home directories (`.claude`, `.codex`, `.gemini`, ...) are checked per path
    /// component first, so a Claude project folder that merely *mentions* another tool
    /// (e.g. `~/.claude/projects/-home-me-gemini-app/`) is still attributed to Claude.
    /// Loose substring matching is the fallback for custom paths like `D:\claude_logs`.
    fn adapter_by_path_keyword(&self, path: &Path) -> Option<&dyn AgentLogAdapter> {
        for comp in path.components() {
            let c = comp.as_os_str().to_string_lossy().to_lowercase();
            match c.as_str() {
                ".claude" => return self.by_id("claude"),
                ".codex" | ".openai" | ".chatgpt" => return self.by_id("chatgpt_codex"),
                ".gemini" | ".antigravity" => return self.by_id("antigravity"),
                _ => {}
            }
        }
        let path_str = path.to_string_lossy().to_lowercase();
        if path_str.contains("claude") {
            return self.by_id("claude");
        }
        if path_str.contains("codex") || path_str.contains("openai") || path_str.contains("chatgpt") {
            return self.by_id("chatgpt_codex");
        }
        if path_str.contains("gemini") || path_str.contains("antigravity") {
            return self.by_id("antigravity");
        }
        None
    }

    pub fn match_adapter_for_path(&self, path: &Path) -> Option<&dyn AgentLogAdapter> {
        self.adapter_by_path_keyword(path).or_else(|| {
            // Fallback: first adapter that accepts the file format
            self.adapters
                .iter()
                .find(|a| a.is_session_file(path))
                .map(|a| a.as_ref())
        })
    }

    /// Classifies a log record. When the owning tool is known from the path, that adapter
    /// is authoritative (its "ignore this record" decision is respected); otherwise every
    /// adapter is tried in turn (auto-detection for custom watch paths and fixtures).
    pub fn classify(&self, path: &Path, record: &Value) -> Option<(AgentState, Option<String>, &'static str)> {
        if let Some(owner) = self.adapter_by_path_keyword(path) {
            return owner
                .classify_record(record)
                .map(|(s, d)| (s, d, owner.id()));
        }
        self.adapters.iter().find_map(|adapter| {
            adapter
                .classify_record(record)
                .map(|(s, d)| (s, d, adapter.id()))
        })
    }
}

// ---------------------------------------------------------------------------
// Watcher Engine & Tail
// ---------------------------------------------------------------------------

/// Where to start reading when switching the tail to a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StartAt {
    /// Skip everything already in the file (used for the session that is already active at app start).
    End,
    /// Continue where we left off if we tailed this file before; otherwise read a small (new)
    /// file from the beginning so its first prompt is not missed.
    Resume,
}

/// Incrementally reads complete JSON lines appended to the active session file.
struct Tail {
    file: Option<PathBuf>,
    position: u64,
    /// Last read offset of previously tailed files, so that alternating between two
    /// concurrently active sessions does not drop the events written in between.
    known_positions: HashMap<PathBuf, u64>,
}

impl Tail {
    fn new() -> Self {
        Self {
            file: None,
            position: 0,
            known_positions: HashMap::new(),
        }
    }

    fn switch_to(&mut self, path: PathBuf, start: StartAt) {
        if let Some(prev) = self.file.take() {
            if self.known_positions.len() > 256 {
                self.known_positions.clear();
            }
            self.known_positions.insert(prev, self.position);
        }
        let len = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        self.position = match start {
            StartAt::End => len,
            StartAt::Resume => match self.known_positions.get(&path) {
                Some(&pos) if pos <= len && len - pos <= NEW_SESSION_REPLAY_LIMIT => pos,
                Some(_) => len,
                None if len <= NEW_SESSION_REPLAY_LIMIT => 0,
                None => len,
            },
        };
        self.file = Some(path);
    }

    /// Returns records from complete lines appended since the last call.
    /// A trailing line without `\n` is still being written and is left for the next poll.
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
            // File was truncated or replaced: start over.
            self.position = 0;
        }
        if len == self.position || file.seek(SeekFrom::Start(self.position)).is_err() {
            return Vec::new();
        }
        let mut bytes = Vec::new();
        if file.read_to_end(&mut bytes).is_err() {
            return Vec::new();
        }
        let Some(last_newline) = bytes.iter().rposition(|&b| b == b'\n') else {
            return Vec::new();
        };
        let complete = &bytes[..=last_newline];
        self.position += complete.len() as u64;

        String::from_utf8_lossy(complete)
            .lines()
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
/// `session_count` is left at 0 here and filled in by [`scan_watch_roots`].
pub fn resolve_all_watch_directories(
    registry: &AgentRegistry,
    custom_paths: &[String],
) -> Vec<WatchedPathInfo> {
    let mut candidates: Vec<(PathBuf, &str)> = Vec::new();

    // 1. Custom Paths from app_config.json
    for cp in custom_paths {
        candidates.push((PathBuf::from(cp), "custom"));
    }

    // 2. System Environment Variable: SCREENMATE_WATCH_DIRS
    if let Ok(env_dirs) = std::env::var("SCREENMATE_WATCH_DIRS") {
        for part in env_dirs.split(';') {
            let trimmed = part.trim();
            if !trimmed.is_empty() {
                candidates.push((PathBuf::from(trimmed), "env"));
            }
        }
    }

    // 3. Defaults from all adapters
    for adapter in registry.adapters() {
        for p in adapter.default_watch_dirs() {
            candidates.push((p, "default"));
        }
    }

    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter_map(|(p, source)| {
            let key = p.to_string_lossy().to_string();
            seen.insert(key.clone()).then(|| WatchedPathInfo {
                path: key,
                source: source.to_string(),
                exists: p.exists(),
                session_count: 0,
            })
        })
        .collect()
}

fn is_candidate_session_file(name: &str) -> bool {
    (name.ends_with(".jsonl") || name.ends_with(".json"))
        && name != "transcript_full.jsonl"
        && name != "character.json"
        && name != "app_config.json"
}

/// Walks every existing watch root once, updating each root's `session_count`
/// (`.jsonl` files) and returning the most recently modified session file overall.
fn scan_watch_roots(paths: &mut [WatchedPathInfo]) -> Option<PathBuf> {
    fn walk(dir: &Path, best: &mut Option<(PathBuf, SystemTime)>, count: &mut usize, depth: usize) {
        if depth > MAX_SCAN_DEPTH {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_dir() {
                walk(&path, best, count, depth + 1);
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !is_candidate_session_file(&name) {
                continue;
            }
            if name.ends_with(".jsonl") {
                *count += 1;
            }
            let Ok(mtime) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            if best.as_ref().is_none_or(|(_, best_mtime)| mtime > *best_mtime) {
                *best = Some((path, mtime));
            }
        }
    }

    let mut best = None;
    for info in paths.iter_mut() {
        let mut count = 0;
        if info.exists {
            walk(Path::new(&info.path), &mut best, &mut count, 0);
        }
        info.session_count = count;
    }
    best.map(|(path, _)| path)
}

/// Returns the most recently modified session file under the given roots (single walk).
#[cfg_attr(not(test), allow(dead_code))]
fn find_latest_session_file(paths: &[WatchedPathInfo]) -> Option<PathBuf> {
    scan_watch_roots(&mut paths.to_vec())
}

fn emit_state(app: &AppHandle, state: AgentState, detail: Option<String>) {
    let _ = app.emit("state-change", StateChangePayload { state, detail });
}

pub async fn run(app: AppHandle, get_custom_paths_fn: impl Fn() -> Vec<String> + Send + 'static) {
    let registry = AgentRegistry::new();
    let mut tail = Tail::new();
    let mut current_state = AgentState::Idle;
    let mut last_activity: Option<Instant> = None;
    let mut watch_info: Vec<WatchedPathInfo> = Vec::new();
    let mut last_config_refresh: Option<Instant> = None;
    let mut last_rescan: Option<Instant> = None;
    let mut scanned_once = false;

    println!("[session_watcher] Multi-agent registry initialized with adapters:");
    for a in registry.adapters() {
        println!("  ✓ [{}] {}", a.id(), a.display_name());
    }

    loop {
        let now = Instant::now();

        // Re-read configured watch roots (cheap: one small JSON file).
        if last_config_refresh.is_none_or(|t| now.duration_since(t) >= CONFIG_REFRESH_INTERVAL) {
            last_config_refresh = Some(now);
            let custom_paths = get_custom_paths_fn();
            let resolved = resolve_all_watch_directories(&registry, &custom_paths);
            let roots_changed = resolved.len() != watch_info.len()
                || resolved.iter().zip(&watch_info).any(|(a, b)| a.path != b.path || a.exists != b.exists);
            if roots_changed {
                watch_info = resolved;
                last_rescan = None; // pick up new roots immediately
            }
        }

        // Discover the newest session file (one directory walk per RESCAN_INTERVAL).
        if last_rescan.is_none_or(|t| now.duration_since(t) >= RESCAN_INTERVAL) {
            let is_first_scan = !scanned_once;
            scanned_once = true;
            last_rescan = Some(now);
            let latest = scan_watch_roots(&mut watch_info);
            update_agent_status(|status| {
                status.watched_paths = watch_info.clone();
            });

            if let Some(latest) = latest {
                if tail.file.as_deref() != Some(latest.as_path()) {
                    println!(
                        "[session_watcher] Active session detected: {}",
                        latest.display()
                    );
                    // The session already active when the app starts is joined at its end;
                    // sessions that appear later are read from their first line.
                    let start = if is_first_scan { StartAt::End } else { StartAt::Resume };
                    tail.switch_to(latest.clone(), start);

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
                    emit_state(&app, current_state, detail);
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
                emit_state(&app, current_state, None);
            } else if elapsed > IDLE_TIMEOUT
                && elapsed <= SLEEP_TIMEOUT
                && current_state != AgentState::Idle
                && current_state != AgentState::Sleeping
            {
                current_state = AgentState::Idle;
                println!("[session_watcher] idle timeout -> Idle");
                emit_state(&app, current_state, None);
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(test)]
#[path = "session_watcher_tests.rs"]
mod tests;
