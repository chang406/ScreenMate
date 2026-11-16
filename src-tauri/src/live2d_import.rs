//! Smart Live2D importer: finds the `.model3.json` in a user-picked folder, collects motion
//! groups / expressions (including loose files not listed in the model), suggests which motion
//! and expression to use for each of the 7 states (multilingual keyword matching), and copies
//! the model into `custom_characters/<id>/` with ASCII-normalised file names.

use crate::characters::validate_character_id;
use crate::config::get_src_dir;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

/// Checks if a PNG texture file has 4K resolution (width or height >= 3840)
pub(crate) fn is_png_4k(path: &Path) -> bool {
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
pub(crate) fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<(), String> {
    fs::create_dir_all(&dst).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Opens a native folder selection dialog.
#[tauri::command]
pub fn pick_folder() -> Result<Option<String>, String> {
    let folder = rfd::FileDialog::new()
        .set_title("Live2D 모델 폴더 선택")
        .pick_folder();
    Ok(folder.map(|p| p.to_string_lossy().to_string()))
}

/// Recursively searches for *.model3.json up to max_depth levels.
pub(crate) fn find_model3_json(dir: &Path, max_depth: usize) -> Option<PathBuf> {
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

const MOTION_SUFFIX: &str = ".motion3.json";
const EXPRESSION_SUFFIX: &str = ".exp3.json";

/// Recursively finds files ending with `suffix` under `base_dir` (up to `max_depth` levels).
/// Returns `(file name without suffix, path relative to base_dir with '/' separators)`.
pub(crate) fn find_files_with_suffix(
    base_dir: &Path,
    suffix: &str,
    max_depth: usize,
) -> Vec<(String, String)> {
    fn walk(base: &Path, dir: &Path, suffix: &str, depth: usize, out: &mut Vec<(String, String)>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                subdirs.push(path);
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if let (Some(stem), Ok(rel)) = (name.strip_suffix(suffix), path.strip_prefix(base)) {
                out.push((stem.to_string(), rel.to_string_lossy().replace('\\', "/")));
            }
        }
        // Files of a folder before its sub-folders (same order as before the refactor).
        for sub in subdirs {
            walk(base, &sub, suffix, depth - 1, out);
        }
    }
    let mut out = Vec::new();
    if base_dir.is_dir() {
        walk(base_dir, base_dir, suffix, max_depth, &mut out);
    }
    out
}

/// Matches a candidate from list using multi-level multilingual keyword groups.
pub(crate) fn match_keyword_thesaurus(
    candidates: &[String],
    keyword_groups: &[&[&str]],
    fallback: &str,
) -> String {
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

// ---------------------------------------------------------------------------
// Keyword thesaurus (English, Korean, Japanese, Chinese) used to guess which motion /
// expression fits each state. Earlier groups win; within a group the first candidate that
// contains any keyword is chosen.
// ---------------------------------------------------------------------------

const IDLE_MOTION_KW: &[&[&str]] = &[
    &[
        "idle", "stand", "wait", "default", "loop", "scene1", "normal", "main",
    ],
    &["대기", "기본", "숨", "루프", "평상"],
    &["待機", "通常", "デフォルト", "ポーズ", "ループ"],
    &["待机", "默认", "平时", "循环"],
];
const THINK_MOTION_KW: &[&[&str]] = &[
    &[
        "think", "look", "search", "question", "nod", "haoqi", "wonder", "curious",
    ],
    &["생각", "고민", "호기심", "분석", "탐색"],
    &["考える", "思考", "疑問", "見渡す", "頷く"],
    &["思考", "好奇", "疑问", "看看", "点头"],
];
const BUSY_MOTION_KW: &[&[&str]] = &[
    &[
        "busy", "work", "act", "magic", "skill", "attack", "zhentou", "scene",
    ],
    &["작업", "집중", "코딩", "스킬", "공격", "베개", "일"],
    &["作業", "仕事", "攻撃", "スキル", "魔法", "枕"],
    &["工作", "干活", "枕头", "抱枕", "施法", "攻击"],
];
const TALK_MOTION_KW: &[&[&str]] = &[
    &[
        "talk", "speak", "cheer", "win", "joy", "happy", "qizi", "smile", "done",
    ],
    &["완료", "성공", "기쁨", "대화", "미소", "깃발", "말하기"],
    &["会話", "喜び", "勝利", "話す", "笑顔", "旗", "完了"],
    &["说话", "开心", "胜利", "聊天", "旗子", "完成"],
];
const CLICK_MOTION_KW: &[&[&str]] = &[
    &["tap", "touch", "click", "react", "yaotou", "poke", "hit"],
    &["터치", "클릭", "반응", "도리도리", "쓰담", "반기기"],
    &["タッチ", "クリック", "反応", "頭揺らし", "照れ", "つつく"],
    &["摇头", "点击", "触摸", "互动", "戳"],
];
const ERROR_MOTION_KW: &[&[&str]] = &[
    &[
        "error", "fail", "bad", "sad", "shock", "linghun", "cry", "panic",
    ],
    &["에러", "실패", "놀람", "영혼", "슬픔", "울음", "당황"],
    &["エラー", "失敗", "驚き", "ショック", "魂", "泣き", "困る"],
    &["灵魂", "失败", "错误", "难过", "震惊", "慌张"],
];
const SLEEP_MOTION_KW: &[&[&str]] = &[
    &["sleep", "rest", "down", "keshui", "bed", "zzz", "lay"],
    &["수면", "잠", "휴식", "졸림", "꾸벅", "눕기"],
    &["睡眠", "寝る", "おやすみ", "休憩", "居眠り", "横になる"],
    &["瞌睡", "睡觉", "休息", "晚安", "躺下"],
];
const IDLE_EXP_KW: &[&[&str]] = &[
    &["normal", "default", "idle", "standard"],
    &["기본", "평상시", "보통"],
    &["通常", "デフォルト", "普通"],
    &["平时", "默认", "正常"],
];
const THINK_EXP_KW: &[&[&str]] = &[
    &[
        "star_eyes",
        "star",
        "think",
        "look",
        "question",
        "haoqi",
        "sparkle",
        "wonder",
    ],
    &["별눈", "반짝", "생각", "호기심", "물음", "고민"],
    &["キラキラ", "星目", "考える", "疑問", "見つめる", "星"],
    &["星星眼", "星星", "好奇", "思考", "疑问", "闪亮"],
];
const BUSY_EXP_KW: &[&[&str]] = &[
    &[
        "left_hand",
        "right_hand",
        "hand",
        "work",
        "busy",
        "baozhen",
        "focus",
        "serious",
    ],
    &["손", "오른손", "왼손", "집중", "포옹", "진지"],
    &["右手", "左手", "手", "集中", "抱き枕", "真面目"],
    &["右手", "左手", "手", "抱枕", "专注", "认真"],
];
const TALK_EXP_KW: &[&[&str]] = &[
    &[
        "heart_eyes",
        "heart",
        "smile",
        "happy",
        "joy",
        "star_eyes",
        "qizi",
        "cheer",
    ],
    &["하트눈", "하트", "미소", "기쁨", "웃음", "깃발", "행복"],
    &[
        "ハート",
        "笑顔",
        "笑い",
        "喜び",
        "目ハート",
        "旗",
        "ハッピー",
    ],
    &["爱心眼", "爱心", "笑", "旗子", "开心", "高兴"],
];
const CLICK_EXP_KW: &[&[&str]] = &[
    &[
        "blush",
        "heart_eyes",
        "shy",
        "smile",
        "heart",
        "react",
        "wink",
    ],
    &["볼붉힘", "부끄", "하트", "미소", "홍조", "윙크"],
    &["照れ", "赤面", "恥ずかしい", "ハート", "笑顔", "ウインク"],
    &["脸红", "爱心眼", "害羞", "红晕", "眨眼"],
];
const ERROR_EXP_KW: &[&[&str]] = &[
    &[
        "angry",
        "tears",
        "cry",
        "dark_face",
        "white_eyes",
        "blood",
        "sad",
        "shock",
        "dizzy",
    ],
    &[
        "화남",
        "분노",
        "눈물",
        "울음",
        "어두움",
        "백안",
        "피",
        "당황",
        "어지러움",
    ],
    &[
        "怒り",
        "涙",
        "泣き",
        "暗い",
        "白目",
        "血",
        "青ざめ",
        "ショック",
    ],
    &["生气", "泪", "白眼", "脸黑", "血", "哭", "怒", "震惊"],
];
const SLEEP_EXP_KW: &[&[&str]] = &[
    &[
        "dark_face",
        "tears",
        "sleep",
        "rest",
        "cry",
        "drowsy",
        "closed",
    ],
    &["어두움", "눈물", "수면", "잠", "졸림", "감은눈"],
    &["暗い", "涙", "眠い", "睡眠", "居眠り", "目を閉じる"],
    &["脸黑", "泪", "瞌睡", "睡觉", "闭眼"],
];

/// Keyword groups for one state (earlier groups win).
type Keywords = &'static [&'static [&'static str]];

/// Motion suggestion rules, in evaluation order: (state, keywords, state to fall back to).
/// `idle` falls back to the first motion group; the others reuse an earlier state's choice.
const MOTION_RULES: &[(&str, Keywords, Option<&str>)] = &[
    ("idle", IDLE_MOTION_KW, None),
    ("thinking", THINK_MOTION_KW, Some("idle")),
    ("busy", BUSY_MOTION_KW, Some("thinking")),
    ("talking", TALK_MOTION_KW, Some("idle")),
    ("click_react", CLICK_MOTION_KW, Some("talking")),
    ("error", ERROR_MOTION_KW, Some("thinking")),
    ("sleeping", SLEEP_MOTION_KW, Some("idle")),
];

/// Expression suggestion rules (no fallback: an unmatched state keeps its current expression).
const EXPRESSION_RULES: &[(&str, Keywords)] = &[
    ("idle", IDLE_EXP_KW),
    ("thinking", THINK_EXP_KW),
    ("busy", BUSY_EXP_KW),
    ("talking", TALK_EXP_KW),
    ("click_react", CLICK_EXP_KW),
    ("error", ERROR_EXP_KW),
    ("sleeping", SLEEP_EXP_KW),
];

fn suggest_motions(motion_groups: &[String]) -> HashMap<String, String> {
    let mut mapping: HashMap<String, String> = HashMap::new();
    for (state, keywords, fallback_state) in MOTION_RULES {
        let chosen = match fallback_state {
            Some(prev) => {
                let fallback = mapping.get(*prev).cloned().unwrap_or_default();
                match_keyword_thesaurus(motion_groups, keywords, &fallback)
            }
            // idle: keyword match, otherwise the first group (empty if the model has none)
            None => {
                let matched = match_keyword_thesaurus(motion_groups, keywords, "");
                if matched.is_empty() {
                    motion_groups.first().cloned().unwrap_or_default()
                } else {
                    matched
                }
            }
        };
        mapping.insert(state.to_string(), chosen);
    }
    mapping
}

fn suggest_expressions(expressions: &[String]) -> HashMap<String, String> {
    EXPRESSION_RULES
        .iter()
        .map(|(state, keywords)| {
            (
                state.to_string(),
                match_keyword_thesaurus(expressions, keywords, ""),
            )
        })
        .collect()
}

/// True if any texture referenced by the model is 4K (width or height >= 3840).
fn uses_4k_texture(model_dir: &Path, model_json: &Value) -> bool {
    model_json
        .get("FileReferences")
        .and_then(|f| f.get("Textures"))
        .and_then(Value::as_array)
        .is_some_and(|textures| {
            textures
                .iter()
                .filter_map(Value::as_str)
                .any(|rel| is_png_4k(&model_dir.join(rel)))
        })
}

/// Motion groups declared in model3.json, plus loose `*.motion3.json` files in the folder that
/// are not referenced by any group (each becomes a group named after the file).
fn collect_motion_groups(model_dir: &Path, model_json: &Value) -> Vec<String> {
    let mut groups: Vec<String> = Vec::new();
    let mut referenced: HashSet<String> = HashSet::new();
    if let Some(motions) = model_json
        .get("FileReferences")
        .and_then(|f| f.get("Motions"))
        .and_then(Value::as_object)
    {
        for (group, entries) in motions {
            groups.push(group.clone());
            for file in entries
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|e| e.get("File").and_then(Value::as_str))
            {
                referenced.insert(file.replace('\\', "/"));
            }
        }
    }
    for (name, rel_path) in find_files_with_suffix(model_dir, MOTION_SUFFIX, 3) {
        if !referenced.contains(&rel_path) && !groups.contains(&name) {
            groups.push(name);
        }
    }
    groups
}

/// Expression names declared in model3.json, plus loose `*.exp3.json` files not referenced there.
fn collect_expressions(model_dir: &Path, model_json: &Value) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let mut referenced: HashSet<String> = HashSet::new();
    for item in model_json
        .get("FileReferences")
        .and_then(|f| f.get("Expressions"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(name) = item.get("Name").and_then(Value::as_str) {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        if let Some(file) = item.get("File").and_then(Value::as_str) {
            referenced.insert(file.replace('\\', "/"));
        }
    }
    for (name, rel_path) in find_files_with_suffix(model_dir, EXPRESSION_SUFFIX, 3) {
        if !referenced.contains(&rel_path) && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Parses a Live2D folder to find model3.json, motion groups, and expressions.
#[tauri::command]
pub fn parse_live2d_folder(folder_path: String) -> Result<Live2DParseResult, String> {
    let p = Path::new(&folder_path);
    if !p.exists() || !p.is_dir() {
        return Err("유효한 폴더 경로가 아닙니다.".into());
    }

    // Look for *.model3.json in the directory (up to 3 levels deep)
    let model_file_path = find_model3_json(p, 3)
        .ok_or_else(|| "폴더 또는 하위 폴더 안에 *.model3.json 파일이 없습니다.".to_string())?;

    let model_filename = model_file_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "모델 파일 이름을 읽을 수 없습니다(지원하지 않는 문자).".to_string())?
        .to_string();

    // Actual directory containing the .model3.json
    let actual_model_dir = model_file_path.parent().unwrap_or(p);
    let actual_folder_str = actual_model_dir.to_string_lossy().to_string();

    let content = fs::read_to_string(&model_file_path).map_err(|e| e.to_string())?;
    let json_val: Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    let is_4k = uses_4k_texture(actual_model_dir, &json_val);
    let motion_groups = collect_motion_groups(actual_model_dir, &json_val);
    let expressions = collect_expressions(actual_model_dir, &json_val);
    let mapping = suggest_motions(&motion_groups);
    let exp_mapping = suggest_expressions(&expressions);

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
pub(crate) fn sanitize_live2d_filenames(
    custom_char_dir: &Path,
    id: &str,
    model_filename: &str,
) -> Result<String, String> {
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

    if let Some(file_refs) = model_json
        .get_mut("FileReferences")
        .and_then(|f| f.as_object_mut())
    {
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
                // `prefix.len()` is a byte offset, so compare bytes (not `chars().nth`, which counts
                // characters and never matched for non-ASCII prefixes such as "火火").
                if name.starts_with(prefix)
                    && (name.len() == prefix.len()
                        || name.as_bytes().get(prefix.len()) == Some(&b'.'))
                {
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

/// Adds loose motion / expression files (found in the folder but not listed in model3.json)
/// to the model's `FileReferences`, so they can be played by group / name.
/// Motions: one new group per file (existing group names are kept). Expressions: appended by
/// name unless already present. Returns true if `model_json` was changed.
fn register_loose_assets(
    model_json: &mut Value,
    loose_motions: Vec<(String, String)>,
    loose_exps: Vec<(String, String)>,
) -> bool {
    let Some(file_refs) = model_json
        .get_mut("FileReferences")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let mut modified = false;

    if !loose_motions.is_empty() {
        if !file_refs.get("Motions").is_some_and(Value::is_object) {
            file_refs.insert("Motions".to_string(), json!({}));
        }
        if let Some(motions) = file_refs.get_mut("Motions").and_then(Value::as_object_mut) {
            for (group, rel_path) in loose_motions {
                if !motions.contains_key(&group) {
                    motions.insert(group, json!([{ "File": rel_path }]));
                    modified = true;
                }
            }
        }
    }

    if !loose_exps.is_empty() {
        if !file_refs.get("Expressions").is_some_and(Value::is_array) {
            file_refs.insert("Expressions".to_string(), json!([]));
        }
        if let Some(exps) = file_refs
            .get_mut("Expressions")
            .and_then(Value::as_array_mut)
        {
            let existing: HashSet<String> = exps
                .iter()
                .filter_map(|e| e.get("Name").and_then(Value::as_str).map(str::to_string))
                .collect();
            for (name, rel_path) in loose_exps {
                if !existing.contains(&name) {
                    exps.push(json!({ "Name": name, "File": rel_path }));
                    modified = true;
                }
            }
        }
    }
    modified
}

/// Best effort: registers loose assets of `model_dir` in `model_file` and rewrites it if needed.
/// Failures are logged; the import itself continues (the model still loads, without extras).
fn register_loose_assets_in_file(model_dir: &Path, model_file: &Path) {
    let Some(mut model_json) = fs::read_to_string(model_file)
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
    else {
        return;
    };
    let motions = find_files_with_suffix(model_dir, MOTION_SUFFIX, 3);
    let exps = find_files_with_suffix(model_dir, EXPRESSION_SUFFIX, 3);
    if register_loose_assets(&mut model_json, motions, exps) {
        let written = serde_json::to_string_pretty(&model_json)
            .map_err(|e| e.to_string())
            .and_then(|pretty| fs::write(model_file, pretty).map_err(|e| e.to_string()));
        if let Err(e) = written {
            eprintln!(
                "[live2d_import] failed to update {}: {e}",
                model_file.display()
            );
        }
    }
}

/// Imports and copies a Live2D character to custom_characters/<id> and registers it.
#[tauri::command]
pub fn save_live2d_character(
    app: AppHandle,
    payload: SaveLive2DCharacterPayload,
) -> Result<(), String> {
    validate_character_id(&payload.id)?;
    // The model file name must be a plain file name inside the copied folder.
    if Path::new(&payload.model_filename)
        .file_name()
        .and_then(|n| n.to_str())
        != Some(payload.model_filename.as_str())
    {
        return Err(format!(
            "모델 파일 이름이 올바르지 않습니다: {}",
            payload.model_filename
        ));
    }
    let src_dir = get_src_dir()?;
    let custom_char_dir = src_dir
        .join("characters")
        .join("custom_characters")
        .join(&payload.id);

    // Copy all files from source directory
    copy_dir_all(&payload.source_folder, &custom_char_dir)?;

    // Sanitize non-ASCII / foreign character filenames to standard ASCII id-based filenames
    let effective_model_filename =
        sanitize_live2d_filenames(&custom_char_dir, &payload.id, &payload.model_filename)
            .unwrap_or_else(|_| payload.model_filename.clone());

    // Ensure model3.json has all loose motions and expressions registered
    register_loose_assets_in_file(
        &custom_char_dir,
        &custom_char_dir.join(&effective_model_filename),
    );

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{png_header, scratch, write};

    /// A small Live2D folder: two registered motion groups, one registered expression, plus a
    /// loose motion and a loose expression file that are not listed in model3.json.
    fn sample_live2d_folder() -> PathBuf {
        let dir = scratch("l2d");
        let model = dir.join("hiyori");
        write(
            &model.join("hiyori.model3.json"),
            &json!({
                "Version": 3,
                "FileReferences": {
                    "Moc": "hiyori.moc3",
                    "Textures": ["hiyori.2048/texture_00.png"],
                    "Motions": {
                        "Idle": [{ "File": "motions/idle_01.motion3.json" }],
                        "TapBody": [{ "File": "motions/tap_01.motion3.json" }]
                    },
                    "Expressions": [{ "Name": "smile", "File": "exp/smile.exp3.json" }]
                }
            })
            .to_string(),
        );
        for f in [
            "motions/idle_01.motion3.json",
            "motions/tap_01.motion3.json",
            "motions/think_01.motion3.json",
            "exp/smile.exp3.json",
            "exp/angry.exp3.json",
        ] {
            write(&model.join(f), "{}");
        }
        // 4K check reads the PNG header: write a 2048x2048 header.
        fs::create_dir_all(model.join("hiyori.2048")).unwrap();
        fs::write(
            model.join("hiyori.2048/texture_00.png"),
            png_header(2048, 2048),
        )
        .unwrap();
        dir
    }

    #[test]
    fn parse_live2d_folder_collects_motions_expressions_and_suggestions() {
        let dir = sample_live2d_folder();
        let r = parse_live2d_folder(dir.to_string_lossy().to_string()).unwrap();
        assert_eq!(r.model_filename, "hiyori.model3.json");
        assert!(r.model_folder.ends_with("hiyori"));
        assert!(!r.is_4k);

        let mut groups = r.motion_groups.clone();
        groups.sort();
        assert_eq!(groups, vec!["Idle", "TapBody", "think_01"]);
        let mut exps = r.expressions.clone();
        exps.sort();
        assert_eq!(exps, vec!["angry", "smile"]);

        assert_eq!(r.suggested_mapping["idle"], "Idle");
        assert_eq!(r.suggested_mapping["thinking"], "think_01");
        assert_eq!(r.suggested_mapping["click_react"], "TapBody");
        assert_eq!(r.suggested_mapping["sleeping"], "Idle"); // falls back to idle
        assert_eq!(r.suggested_expressions["talking"], "smile");
        assert_eq!(r.suggested_expressions["error"], "angry");
        assert_eq!(r.suggested_expressions["idle"], "");
        assert_eq!(r.suggested_mapping.len(), 7);
        assert_eq!(r.suggested_expressions.len(), 7);
    }

    #[test]
    fn parse_live2d_folder_reports_missing_model() {
        let dir = scratch("l2d_empty");
        assert!(parse_live2d_folder(dir.to_string_lossy().to_string()).is_err());
        assert!(parse_live2d_folder("/definitely/not/here".into()).is_err());
    }

    #[test]
    fn png_4k_detection_reads_header() {
        let dir = scratch("png");
        let mk = png_header;
        fs::write(dir.join("a.png"), mk(4096, 4096)).unwrap();
        fs::write(dir.join("b.png"), mk(2048, 3840)).unwrap();
        fs::write(dir.join("c.png"), mk(2048, 2048)).unwrap();
        fs::write(dir.join("d.png"), b"not a png").unwrap();
        assert!(is_png_4k(&dir.join("a.png")));
        assert!(is_png_4k(&dir.join("b.png")));
        assert!(!is_png_4k(&dir.join("c.png")));
        assert!(!is_png_4k(&dir.join("d.png")));
        assert!(!is_png_4k(&dir.join("missing.png")));
    }

    #[test]
    fn keyword_thesaurus_prefers_earlier_groups_then_fallback() {
        let c: Vec<String> = ["Main_Loop", "待機", "Tap"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            match_keyword_thesaurus(&c, &[&["loop"], &["待機"]], ""),
            "Main_Loop"
        );
        assert_eq!(match_keyword_thesaurus(&c, &[&["nothing"]], "Tap"), "Tap");
        assert_eq!(match_keyword_thesaurus(&c, &[&["nothing"]], "Absent"), "");
    }

    #[test]
    fn sanitize_renames_non_ascii_model_files_to_id() {
        let dir = scratch("sanitize");
        write(
            &dir.join("火火.model3.json"),
            &json!({ "FileReferences": {
                "Moc": "火火.moc3", "Physics": "火火.physics3.json",
                "Textures": ["火火.4096/texture_00.png"] } })
            .to_string(),
        );
        write(&dir.join("火火.moc3"), "moc");
        write(&dir.join("火火.physics3.json"), "{}");
        write(&dir.join("火火.4096/texture_00.png"), "png");
        let new_name = sanitize_live2d_filenames(&dir, "huohuo", "火火.model3.json").unwrap();
        assert_eq!(new_name, "huohuo.model3.json");
        assert!(dir.join("huohuo.moc3").exists());
        assert!(dir.join("huohuo.4096/texture_00.png").exists());
        assert!(!dir.join("火火.model3.json").exists());
        let v: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("huohuo.model3.json")).unwrap())
                .unwrap();
        assert_eq!(v["FileReferences"]["Moc"], "huohuo.moc3");
        assert_eq!(
            v["FileReferences"]["Textures"][0],
            "huohuo.4096/texture_00.png"
        );
        // Already-clean names are left alone.
        assert_eq!(
            sanitize_live2d_filenames(&dir, "huohuo", "huohuo.model3.json").unwrap(),
            "huohuo.model3.json"
        );
    }

    #[test]
    fn find_files_with_suffix_returns_stems_and_relative_paths() {
        let dir = scratch("find_suffix");
        write(&dir.join("a.motion3.json"), "{}");
        write(&dir.join("sub/b.motion3.json"), "{}");
        write(&dir.join("sub/deep/deeper/c.motion3.json"), "{}"); // depth 4 > 3
        write(&dir.join("sub/x.exp3.json"), "{}");
        let mut found = find_files_with_suffix(&dir, MOTION_SUFFIX, 3);
        found.sort();
        assert_eq!(
            found,
            vec![
                ("a".to_string(), "a.motion3.json".to_string()),
                ("b".to_string(), "sub/b.motion3.json".to_string()),
            ]
        );
        assert_eq!(find_files_with_suffix(&dir, EXPRESSION_SUFFIX, 3).len(), 1);
        assert!(find_files_with_suffix(&dir.join("missing"), MOTION_SUFFIX, 3).is_empty());
    }

    #[test]
    fn motion_suggestions_follow_fallback_chain_without_keywords() {
        // No keyword matches at all: idle = first group, everything else chains back to it.
        let groups = vec!["Group_A".to_string(), "Group_B".to_string()];
        let m = suggest_motions(&groups);
        for state in [
            "idle",
            "thinking",
            "busy",
            "talking",
            "click_react",
            "error",
            "sleeping",
        ] {
            assert_eq!(m[state], "Group_A", "{state}");
        }
        assert!(suggest_motions(&[]).values().all(String::is_empty));
    }

    #[test]
    fn loose_assets_are_registered_without_overwriting_existing_ones() {
        let mut model = json!({ "FileReferences": {
            "Motions": { "Idle": [{ "File": "m/idle.motion3.json" }] },
            "Expressions": [{ "Name": "smile", "File": "e/smile.exp3.json" }]
        }});
        let changed = register_loose_assets(
            &mut model,
            vec![
                ("Idle".into(), "m/other_idle.motion3.json".into()), // group exists -> kept
                ("wave".into(), "m/wave.motion3.json".into()),
            ],
            vec![
                ("smile".into(), "e/x.exp3.json".into()),
                ("angry".into(), "e/angry.exp3.json".into()),
            ],
        );
        assert!(changed);
        let refs = &model["FileReferences"];
        assert_eq!(refs["Motions"]["Idle"][0]["File"], "m/idle.motion3.json");
        assert_eq!(refs["Motions"]["wave"][0]["File"], "m/wave.motion3.json");
        assert_eq!(refs["Expressions"].as_array().unwrap().len(), 2);
        assert_eq!(refs["Expressions"][1]["Name"], "angry");

        // Nothing new -> unchanged; no FileReferences -> untouched.
        assert!(!register_loose_assets(&mut model, vec![], vec![]));
        let mut bare = json!({});
        assert!(!register_loose_assets(
            &mut bare,
            vec![("x".into(), "x".into())],
            vec![]
        ));
        assert_eq!(bare, json!({}));
    }
}
