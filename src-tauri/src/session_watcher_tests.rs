//! Unit tests for `session_watcher` (agent log classification & tailing).
//! Fixture files live in `tests/fixtures/agents/` at the project root.

use super::*;
use serde_json::json;
use std::io::Write;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("agents")
}

/// Unique scratch directory under the system temp dir (cleaned by the OS; no extra deps).
fn scratch_dir(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("screenmate_test_{}_{}_{}", tag, std::process::id(), nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Classifies every line of a fixture as if it were tailed from `virtual_path`.
fn replay(virtual_path: &str, fixture: &Path) -> Vec<(AgentState, Option<String>)> {
    let registry = AgentRegistry::new();
    let content = fs::read_to_string(fixture).expect("fixture readable");
    content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter_map(|v| registry.classify(Path::new(virtual_path), &v))
        .map(|(s, d, _)| (s, d))
        .collect()
}

fn classify_as(virtual_path: &str, record: Value) -> Option<(AgentState, Option<String>)> {
    AgentRegistry::new()
        .classify(Path::new(virtual_path), &record)
        .map(|(s, d, _)| (s, d))
}

const CLAUDE_PATH: &str = "/home/u/.claude/projects/my-proj/session.jsonl";
const CODEX_PATH: &str = "/home/u/.codex/sessions/2026/11/16/rollout-abc.jsonl";
const AGY_PATH: &str = "/home/u/.gemini/antigravity-ide/brain/x/transcript.jsonl";

fn busy(detail: &str) -> (AgentState, Option<String>) {
    (AgentState::Busy, Some(detail.to_string()))
}

// ---------------------------------------------------------------------------
// Existing fixtures (regression guard for the original sample sessions)
// ---------------------------------------------------------------------------

#[test]
fn antigravity_fixture_sequence() {
    let got = replay(AGY_PATH, &fixtures_dir().join("antigravity/sample_session.jsonl"));
    assert_eq!(
        got,
        vec![
            (AgentState::Thinking, None),
            busy("edit"),
            busy("general"),
            (AgentState::Talking, None),
        ]
    );
}

#[test]
fn claude_fixture_sequence() {
    let got = replay(CLAUDE_PATH, &fixtures_dir().join("claude/sample_session.jsonl"));
    assert_eq!(
        got,
        vec![
            (AgentState::Thinking, None),
            busy("read"),
            busy("general"),
            (AgentState::Talking, None),
        ]
    );
}

#[test]
fn codex_legacy_fixture_sequence() {
    let got = replay(CODEX_PATH, &fixtures_dir().join("chatgpt_codex/sample_session.jsonl"));
    assert_eq!(
        got,
        vec![
            (AgentState::Thinking, None),
            busy("command"),
            busy("general"),
            busy("edit"),
            (AgentState::Talking, None),
        ]
    );
}

// ---------------------------------------------------------------------------
// Claude Code real-world record shapes
// ---------------------------------------------------------------------------

#[test]
fn claude_tool_result_inside_user_record_is_busy() {
    // Claude Code writes tool results as `type: "user"` records with tool_result blocks.
    let rec = json!({
        "type": "user",
        "message": { "role": "user", "content": [
            { "type": "tool_result", "tool_use_id": "toolu_1", "content": "ok" }
        ]}
    });
    assert_eq!(classify_as(CLAUDE_PATH, rec), Some(busy("general")));
}

#[test]
fn claude_plain_user_prompt_is_thinking() {
    let rec = json!({
        "type": "user",
        "message": { "role": "user", "content": "버그 고쳐줘" }
    });
    assert_eq!(classify_as(CLAUDE_PATH, rec), Some((AgentState::Thinking, None)));
}

#[test]
fn claude_api_error_message_is_error() {
    let rec = json!({
        "type": "assistant",
        "isApiErrorMessage": true,
        "message": { "role": "assistant", "content": [
            { "type": "text", "text": "API Error: 529 Overloaded" }
        ]}
    });
    assert_eq!(classify_as(CLAUDE_PATH, rec), Some((AgentState::Error, None)));
}

#[test]
fn claude_meta_records_are_ignored() {
    for rec in [
        json!({ "type": "summary", "summary": "Fix login bug", "leafUuid": "x" }),
        json!({ "type": "user", "isMeta": true, "message": { "role": "user", "content": "<command-name>/clear</command-name>" } }),
        json!({ "type": "file-history-snapshot", "snapshot": {} }),
    ] {
        assert_eq!(classify_as(CLAUDE_PATH, rec.clone()), None, "record should be ignored: {}", rec);
    }
}

#[test]
fn claude_realistic_fixture_sequence() {
    let got = replay(CLAUDE_PATH, &fixtures_dir().join("claude/realistic_session.jsonl"));
    assert_eq!(
        got,
        vec![
            (AgentState::Thinking, None), // user prompt
            (AgentState::Thinking, None), // extended thinking block
            busy("read"),                 // Read tool
            busy("general"),              // tool_result (user record)
            busy("edit"),                 // Edit tool
            busy("general"),              // tool_result (user record)
            busy("command"),              // Bash tool
            busy("general"),              // tool_result
            (AgentState::Talking, None),  // final answer
        ]
    );
}

// ---------------------------------------------------------------------------
// OpenAI Codex CLI rollout format (`~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`)
// ---------------------------------------------------------------------------

#[test]
fn codex_rollout_records() {
    let cases = vec![
        (
            json!({ "type": "response_item", "payload": { "type": "message", "role": "user",
                "content": [{ "type": "input_text", "text": "테스트 고쳐줘" }] } }),
            Some((AgentState::Thinking, None)),
        ),
        (
            json!({ "type": "response_item", "payload": { "type": "reasoning", "summary": [] } }),
            Some((AgentState::Thinking, None)),
        ),
        (
            json!({ "type": "response_item", "payload": { "type": "function_call", "name": "shell",
                "arguments": "{\"command\":[\"cargo\",\"test\"]}", "call_id": "c1" } }),
            Some(busy("command")),
        ),
        (
            json!({ "type": "response_item", "payload": { "type": "function_call_output", "call_id": "c1",
                "output": "ok" } }),
            Some(busy("general")),
        ),
        (
            json!({ "type": "event_msg", "payload": { "type": "agent_message", "message": "완료했습니다" } }),
            Some((AgentState::Talking, None)),
        ),
        (json!({ "type": "session_meta", "payload": { "id": "x", "cwd": "/tmp" } }), None),
        (json!({ "type": "event_msg", "payload": { "type": "token_count", "info": null } }), None),
    ];
    for (rec, expected) in cases {
        assert_eq!(classify_as(CODEX_PATH, rec.clone()), expected, "record: {}", rec);
    }
}

#[test]
fn codex_rollout_fixture_sequence() {
    let got = replay(CODEX_PATH, &fixtures_dir().join("chatgpt_codex/rollout_session.jsonl"));
    assert_eq!(
        got,
        vec![
            (AgentState::Thinking, None), // user message
            (AgentState::Thinking, None), // reasoning
            busy("command"),              // shell
            busy("general"),              // function_call_output
            busy("edit"),                 // apply_patch
            busy("general"),              // function_call_output
            (AgentState::Talking, None),  // assistant message
        ]
    );
}

// ---------------------------------------------------------------------------
// Tail: incremental reading of a file that is being appended to
// ---------------------------------------------------------------------------

fn append(path: &Path, bytes: &[u8]) {
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
    f.write_all(bytes).unwrap();
    f.flush().unwrap();
}

/// Starts tailing `path` from its current end (the behaviour for an already-active file).
fn tail_from_end(path: &Path) -> Tail {
    let mut tail = Tail::new();
    tail.switch_to(path.to_path_buf());
    tail
}

#[test]
fn tail_reads_appended_complete_lines() {
    let dir = scratch_dir("tail_complete");
    let file = dir.join("s.jsonl");
    append(&file, b"{\"old\":1}\n");
    let mut tail = tail_from_end(&file);
    assert!(tail.read_new_records().is_empty(), "pre-existing content must be skipped");

    append(&file, b"{\"a\":1}\n{\"b\":2}\n");
    let recs = tail.read_new_records();
    assert_eq!(recs, vec![json!({"a":1}), json!({"b":2})]);
}

#[test]
fn tail_does_not_drop_partially_written_line() {
    let dir = scratch_dir("tail_partial");
    let file = dir.join("s.jsonl");
    append(&file, b"");
    let mut tail = tail_from_end(&file);

    // Writer flushed only half of a JSON line.
    append(&file, b"{\"type\":\"user\",\"mess");
    assert!(tail.read_new_records().is_empty());

    // The rest arrives on the next poll — the record must not be lost.
    append(&file, b"age\":\"hi\"}\n");
    assert_eq!(tail.read_new_records(), vec![json!({"type":"user","message":"hi"})]);
}

#[test]
fn tail_survives_split_multibyte_utf8() {
    let dir = scratch_dir("tail_utf8");
    let file = dir.join("s.jsonl");
    append(&file, b"");
    let mut tail = tail_from_end(&file);

    let line = "{\"t\":\"완료\"}\n".as_bytes();
    let cut = line.iter().position(|&b| b >= 0x80).unwrap() + 1; // inside the first Hangul char
    append(&file, &line[..cut]);
    let _ = tail.read_new_records();
    append(&file, &line[cut..]);
    let mut all = tail.read_new_records();
    all.extend(tail.read_new_records());
    assert_eq!(all, vec![json!({"t":"완료"})]);
}

#[test]
fn tail_resets_when_file_is_truncated() {
    let dir = scratch_dir("tail_trunc");
    let file = dir.join("s.jsonl");
    append(&file, b"{\"x\":1}\n{\"x\":2}\n");
    let mut tail = tail_from_end(&file);
    fs::write(&file, b"{\"y\":1}\n").unwrap();
    assert_eq!(tail.read_new_records(), vec![json!({"y":1})]);
}

// ---------------------------------------------------------------------------
// Session discovery cost (run manually: `cargo test -- --ignored --nocapture`)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn bench_find_latest_session_file() {
    let dir = scratch_dir("bench_scan");
    for p in 0..40 {
        let proj = dir.join(format!("project-{p}"));
        fs::create_dir_all(&proj).unwrap();
        for s in 0..50 {
            fs::write(proj.join(format!("session-{s}.jsonl")), b"{}\n").unwrap();
        }
    }
    let info = vec![WatchedPathInfo {
        path: dir.to_string_lossy().to_string(),
        source: "custom".into(),
        exists: true,
        session_count: 0,
    }];
    let iters = 50;
    let start = Instant::now();
    for _ in 0..iters {
        assert!(find_latest_session_file(&info).is_some());
    }
    let per_call = start.elapsed() / iters;
    let calls_per_sec = 1000.0 / POLL_INTERVAL.as_millis() as f64;
    println!(
        "[bench] 2000 files: {:?} per full scan; at 150ms polling = {:.1} scans/s => ~{:.1} ms CPU/s spent scanning",
        per_call,
        calls_per_sec,
        per_call.as_secs_f64() * 1000.0 * calls_per_sec
    );
}
