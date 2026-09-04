# 📄 PRD & Plan: 멀티 환경 커스텀 감시 경로 및 AI 에이전트 연결 상태 모니터링

- **문서 버전**: v1.0.0
- **상태**: Draft (기획 완료 및 구현 준비)
- **대상 모듈**: `src-tauri/src/session_watcher.rs`, `src/app_config.json`, `src/settings.html`, `src/settings.js`

---

## 1. 배경 및 문제 정의 (Background & Problem Statement)

### ① 배경
ScreenMate는 OS 파일 시스템 레벨에서 AI 코딩 도구(Claude Code, Antigravity IDE, Antigravity CLI)가 기록하는 대화 세션 로그(`transcript.jsonl`)를 150ms 주기로 추적하여 캐릭터의 표정과 모션을 실시간 제어합니다.

### ② 문제점 (Pain Points)
1. **특수 환경 및 드라이브 이전 사용자 대응 한계**:
   - 현재 백엔드는 `dirs::home_dir()`을 기준으로 기본 경로(`~/.claude/projects/`, `~/.gemini/...`) 4곳만 고정 탐색합니다.
   - SSD 용량 관리, 사내 보안 정책, 포터블 환경 등으로 인해 AI 작업 공간이나 로그 폴더를 **D: 드라이브, E: 드라이브, 또는 별도 사용자 지정 경로**로 옮긴 사용자는 세션 감지가 작동하지 않습니다.
2. **침묵 실패(Silent Failure)로 인한 UX 저하**:
   - AI 로그 경로를 찾지 못했거나 활성 세션이 없을 때 사용자에게 아무런 시각적 피드백을 주지 않아, 사용자는 "캐릭터가 왜 반응을 안 하지? 버그인가?" 하고 오해하게 됩니다.
3. **신규 AI 도구 확장의 유연성 부족**:
   - 향후 Cursor, Roo Code, Cline, Windsurf 등 다양한 AI 코딩 에이전트를 지원하려면 코드 수정 없이도 사용자가 직접 로그 경로를 지정할 수 있는 유연한 구조가 필수적입니다.

---

## 2. 목표 및 기대 효과 (Goals & Non-Goals)

### 🎯 목표 (Goals)
1. **경로 유연성**: 사용자가 원하는 임의의 디렉토리(D 드라이브, 외부 볼륨 등)를 감시 목록에 추가/제거할 수 있도록 지원.
2. **상태 가시성**: 현재 어떤 AI 도구의 세션이 활성화되어 있고 연결되어 있는지 캐릭터 매니저 GUI에서 한눈에 확인 가능.
3. **환경 변수 호환**: 개발자 친화적인 표준 환경 변수(`SCREENMATE_WATCH_DIRS`) 지원.
4. **하위 호환성 100%**: 기존 일반 사용자는 아무런 설정 없이도 기본 경로에서 완벽하게 자동 작동 유지.

### 🚫 비목표 (Non-Goals)
- 무제한 전체 드라이브(C:\, D:\ 루트) 재귀 탐색 (성능 저하 및 디스크 I/O 병목 방지).
- AI 도구 프로세스 강제 주입/후킹 (안전한 파일 읽기 관찰자 철학 유지).

---

## 3. 기능 요구사항 명세 (Functional Requirements)

### FR-1. 사용자 정의 감시 경로 설정 (`src/app_config.json`)
- `src/app_config.json`에 `customWatchPaths` 문자열 배열 필드 지원:
  ```json
  {
    "activeCharacter": "huohuo",
    "customWatchPaths": [
      "D:\\DevTools\\claude_projects",
      "E:\\Gemini\\brain"
    ],
    "globalBubbleSettings": { ... }
  }
  ```
- **탐색 우선순위**:
  1. `customWatchPaths` (사용자가 명시한 커스텀 경로)
  2. 시스템 환경 변수 (`SCREENMATE_WATCH_DIRS`)
  3. 기본 OS 홈 디렉토리 (`dirs::home_dir()` 하위 4개 경로)

### FR-2. AI 연결 상태 실시간 진단 API (`get_agent_status`)
- 프론트엔드가 백엔드에 현재 에이전트 연결 상태를 질의할 수 있는 Tauri Command 제공:
  - **응답 데이터 구조**:
    ```json
    {
      "isConnected": true,
      "activeTool": "antigravity", // "antigravity" | "claude" | "custom" | null
      "activeSessionFile": "C:\\Users\\...\\transcript.jsonl",
      "lastEventTime": "2026-09-04T11:15:00Z",
      "watchedPaths": [
        { "path": "C:\\Users\\...\\.gemini\\brain", "exists": true, "sessionCount": 3 },
        { "path": "D:\\MyLogs", "exists": true, "sessionCount": 1 },
        { "path": "E:\\WrongPath", "exists": false, "sessionCount": 0 }
      ]
    }
    ```

### FR-3. 캐릭터 매니저 GUI 내 "AI 연결 및 경로 관리" 섹션
- `settings.html`의 새로운 탭 또는 모달 영역:
  - **상태 카드**:
    - 🟢 **정상 연결됨**: *"Antigravity IDE 활성 세션 감지 중 (최근 반응: 3초 전)"*
    - ⚪ **대기 중**: *"감시 중인 폴더 4개 정상 등록됨 (AI 작업 시작 시 자동 반응)"*
    - 🔴 **경로 오류**: *"등록된 폴더가 존재하지 않습니다."*
  - **커스텀 폴더 추가/삭제 UI**:
    - `[ 📁 폴더 찾기 ]` 버튼으로 D 드라이브나 원하는 디렉토리 원클릭 등록.
    - 등록된 경로 목록 표 및 삭제(x) 버튼.

---

### FR-4. 플러그인형 에이전트 어댑터 아키텍처 (Agent Log Adapter Trait)
신규 AI 에이전트(ChatGPT/Codex, Cursor, Roo Code 등)가 추가될 때마다 기존 코드를 해치지 않고 모듈식으로 확장하기 위한 Rust 트레이트 추상화:

```rust
pub trait AgentLogAdapter: Send + Sync {
    /// 도구 식별자 (예: "antigravity", "claude", "chatgpt_codex", "cursor")
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    
    /// 해당 도구의 기본 로그 저장 디렉토리 목록
    fn default_watch_dirs(&self) -> Vec<PathBuf>;
    
    /// 대상 파일이 이 에이전트의 세션 로그인지 판별
    fn is_session_file(&self, path: &Path) -> bool;
    
    /// 로그 레코드(한 줄 또는 JSON)를 7대 AgentState로 분류
    fn classify_record(&self, line: &str) -> Option<(AgentState, Option<String>)>;
}
```

---

### FR-5. 로컬 미보유 에이전트 개발을 위한 모의 시뮬레이터 (Fixture Replay Engine)
- **배경**: 개발자의 로컬 PC에 특정 AI 도구(예: ChatGPT Desktop, Codex, Cursor 등)가 설치되어 있지 않더라도, 해당 도구의 지원 기능을 개발하고 캐릭터 반응을 100% 검증할 수 있어야 합니다.
- **방식**:
  1. `tests/fixtures/agents/<에이전트명>/sample_session.jsonl`에 커뮤니티나 공식 샘플 로그 저장.
  2. 백엔드에 `test_replay_session` 커맨드를 제공하여, 실제 도구 없이도 가상의 로그 스트림을 재생해 캐릭터 반응(생각 ➡️ 도구실행 ➡️ 말풍선 답변 ➡️ 에러)을 시뮬레이션 검증.

---

### FR-6. 무설치 선언형 JSON 파싱 규칙 (Declarative Custom Agent Rules)
- Rust 코드를 다시 컴파일하지 않고도 사용자가 직접 새 에이전트의 로그 형식을 등록할 수 있는 JSON 규칙 엔진:
  ```json
  {
    "agentId": "chatgpt_codex",
    "name": "ChatGPT Codex CLI",
    "filePattern": "*.jsonl",
    "rules": {
      "thinking": { "field": "role", "equals": "user" },
      "busy": { "field": "function_call", "exists": true },
      "talking": { "field": "role", "equals": "assistant" },
      "error": { "field": "status", "equals": "failed" }
    }
  }
  ```

---

### FR-7. 범용 로컬 HTTP Webhook 수신 엔드포인트 (`localhost:23579/agent/state`)
- 파일 로그가 남지 않는 웹 브라우저 확장(ChatGPT Web)이나 파이썬 스크립트에서도 HTTP 요청 한 번으로 캐릭터를 반응시킬 수 있는 범용 엔드포인트 제공:
  - `POST http://127.0.0.1:23579/agent/state`
  - Body: `{"state": "busy", "detail": "command", "subtitle": "데이터 분석 중!"}`

---

## 4. 기술 아키텍처 및 데이터 흐름

```
[입력 소스들]
  ├── A. 디렉토리 감시 (기본 홈 경로 + D: 드라이브 등 customWatchPaths)
  ├── B. 모의 픽스처 재생기 (tests/fixtures/...)
  └── C. 로컬 HTTP Webhook (포트: 23579)
       ↓
[Agent Registry (플러그인 매니저)]
  ├── 1. AntigravityAdapter
  ├── 2. ClaudeAdapter
  ├── 3. ChatGptCodexAdapter
  └── 4. DeclarativeJsonAdapter (사용자 커스텀 규칙)
       ↓ (어댑터 자동 매칭 및 상태 추출)
[Unified State Machine (Idle/Thinking/Busy/Talking/Error/Sleeping)]
       ↓ (Tauri Event: state-change)
[프론트엔드 캐릭터 애니메이션 & 말풍선 반응]
```

---

## 5. 단계별 구현 계획 (Implementation Plan)

### 📌 Milestone 1: 어댑터 구조 리팩토링 & 커스텀 경로 (예상 소요: 1~2시간)
1. `session_watcher.rs` 내부 로직을 `AgentLogAdapter` 트레이트 기반으로 구조화.
2. `src/app_config.json`의 `customWatchPaths` 동적 감시 병합.
3. 안전한 디렉토리 필터링(D:, E: 드라이브 접근 검증).

### 📌 Milestone 2: 픽스처 재생기(Fixture Replay) & ChatGPT/Codex 어댑터 (예상 소요: 1시간)
1. `tests/fixtures/agents/` 디렉토리에 ChatGPT/Codex 및 Cursor 샘플 로그 배치.
2. 모의 세션 재생기 구현을 통해 로컬 설치 없이도 ChatGPT/Codex 어댑터 동작 검증.

### 📌 Milestone 3: GUI 연결 상태 진단 & 선언형 규칙 확장 (예상 소요: 1~2시간)
1. `settings.html`에 AI 에이전트 연결 상태 뱃지 및 경로 관리 UI 구축.
2. 커스텀 JSON 규칙 등록 지원.
3. (선택) 로컬 Webhook 수신기 통합.

---

## 6. 품질 및 안정성 체크리스트 (Verification)

- [ ] D: 드라이브 등 C: 외의 볼륨 경로를 등록했을 때 정상적으로 파일이 감지되는가?
- [ ] 개발자 PC에 ChatGPT/Codex가 없어도 픽스처 재생기로 파서 동작이 완벽히 검증되는가?
- [ ] 새 에이전트 추가 시 기존 Antigravity/Claude Code 감시 로직에 영향(회귀 버그)이 없는가?
- [ ] 잘못된 로그 라인(깨진 JSON, 알 수 없는 포맷)이 들어와도 안전하게 폴백되는가?

