# ScreenMate 개발 및 에이전트 협업 가이드 (AGENTS.md)

이 문서는 AI 코딩 에이전트(Antigravity, Claude Code, Cursor 등)가 **ScreenMate** 프로젝트를 이해하고, 일관된 아키텍처 규칙과 개발 표준을 유지하며 안전하게 수정할 수 있도록 돕는 지침서입니다.

---

## 1. 프로젝트 개요 (Overview)

- **이름**: ScreenMate (초경량 데스크톱 AI 스크린 메이트)
- **목적**: 사용자가 **Antigravity IDE/CLI** 및 **Claude Code**에서 작업할 때 실시간 세션 로그를 분석하여, 투명 오버레이 윈도우 위에서 귀여운 캐릭터(Live2D 및 2D 이미지/SVG)가 상태에 맞춰 반응(대사, 모션, 표정)하도록 지원.
- **핵심 기술 스택**:
  - **백엔드**: Rust + [Tauri v2](https://v2.tauri.app/) (별도 서버 프로세스 없는 단일 바이너리)
  - **프론트엔드**: Vanilla HTML5/CSS/JavaScript + [Pixi.js v6](https://pixijs.com/) + [pixi-live2d-display](https://github.com/guansss/pixi-live2d-display)
  - **런타임 라이브러리**: Live2D Cubism Core Web SDK

---

## 2. 제품 철학 및 스코프 경계선 (Philosophy & Anti-Patterns)

모든 AI 에이전트는 기능 추가나 코드 수정 시 다음 제품 철학과 스코프 제한을 반드시 준수해야 합니다.

### ① 핵심 철학 (The North Star)
- **"AI 코딩 에이전트의 백그라운드 두뇌 상태를 투명하게 실시간 시각화해 주는 가볍고 귀여운 동반자"**
- 에이전트의 7대 상태(생각, 실행, 완료, 에러 등)에 대한 즉각적이고 매력적인 시청각 반응에 집중합니다.

### ② 명백한 스코프 아웃 (Strictly Out of Scope / Anti-Patterns)
1. ❌ **자체 대화형 AI 챗봇화 금지**:
   - 사용자가 직접 말을 걸고 답변을 받는 챗봇/음성 비서로 확장하지 않습니다. (ScreenMate는 Claude/Gemini의 대체재가 아니라 **감시자/표현자(Observer & Visualizer)**입니다.)
2. ❌ **잡동사니 생산성 위젯화 금지**:
   - 뽀모도로 타이머, 투두리스트, 캘린더, 날씨, 상시 하드웨어(CPU/RAM) 폴링 등 무거운 위젯 기능을 넣지 않습니다.
3. ❌ **침입형(Intrusive) UI 금지**:
   - 개발자의 포커스를 빼앗거나 화면 중앙을 침범하는 강제 팝업창을 띄우지 않습니다.

---

## 3. 빌드 및 실행 명령어 (Commands)

```bash
# 필수: Rust 툴체인 (cargo) 및 C++ Build Tools (Windows)

# 개발 모드 실행 (src-tauri 디렉토리에서 실행)
cd src-tauri
cargo run

# 문법 및 타입 체크
cargo check

# 릴리즈 단일 바이너리 빌드 (선택 사항)
cargo build --release
```

---

## 4. 핵심 아키텍처 및 불변 규칙 (Core Architecture & Rules)

### ① 투명 마우스 클릭 관통 (Per-pixel Click-Through)
- **백엔드 (`src-tauri/src/cursor_watcher.rs`)**: 16ms 주기로 OS 마우스 커서 좌표를 감시하여 프론트엔드로 `cursor-position` 이벤트를 전송.
- **프론트엔드 (`src/main.js`)**:
  - 캔버스/이미지의 알파 채널(투명도)을 실시간 검사.
  - 마우스가 캐릭터 불투명 영역 위에 있으면 `invoke('set_click_through', { ignore: false })` (클릭/드래그 가능).
  - 마우스가 투명 영역이나 빈 공간에 있으면 `invoke('set_click_through', { ignore: true })` (마우스가 뒤의 작업 창으로 통과).
- **창 드래그 (`cursor_watcher.rs` `begin_drag`/`end_drag`)**: OS 기본 창 이동(`startDragging`)은 창 꼭대기가 화면 위로 못 가게 막으므로 쓰지 않습니다. 프론트가 `begin_drag`를 호출하면 16ms 커서 폴링 루프가 커서 이동량만큼 `set_position`으로 창을 옮기고, 마우스를 놓거나(`mouseup`, `buttons==0`, 창 blur) 하면 `end_drag`합니다.
- ⚠️ **주의**: 클릭 관통 로직을 무력화하거나 CSS `pointer-events: none`만으로 대체하지 마십시오 (OS 레벨 클릭 관통이 깨집니다).

### ② 세션 로그 감시 및 다중 에이전트 어댑터 아키텍처 (`session_watcher.rs`)
- **다중 에이전트 어댑터 (`AgentLogAdapter` trait)**:
  - `AntigravityAdapter`: Antigravity IDE(`~/.gemini/antigravity-ide/brain/`) 및 CLI(`~/.gemini/agy/logs/`, `~/.antigravity/logs/`)
  - `ClaudeAdapter`: Claude Code(`~/.claude/projects/`)
  - `ChatGptCodexAdapter`: ChatGPT / Codex CLI 세션 로그 지원
- **탐색 경로 캐스케이딩 (우선순위)**:
  1. `src/app_config.json`의 `customWatchPaths` (D: 드라이브, 외장 SSD 등 사용자 정의 경로)
  2. 시스템 환경 변수 `SCREENMATE_WATCH_DIRS`
  3. 기본 OS 홈 디렉토리 경로
- **모의 시뮬레이터 (Fixture Replay)**:
  - `tests/fixtures/agents/`의 샘플 세션을 통해 로컬에 AI 도구가 미설치되어 있어도 프론트엔드에서 즉시 캐릭터 리액션을 테스트 가능 (`replay_fixture_session`).
- **프라이버시 안심 마스킹 (`maskHomePath`)**:
  - GUI 화면에서 사용자 로컬 경로(`C:\Users\<username>`)는 `~`로 자동 마스킹 표기되며, 마우스 오버 시 실제 전체 경로 툴팁 제공.
  - 커스텀 감시 경로는 `.gitignore`로 보호되는 로컬 `app_config.json`에만 저장되어 깃 푸시 시 노출 원천 차단.
- **테일링 규칙**: 완성된 줄(`\n`)까지만 소비하고 기록 중인 반쪽 줄은 다음 폴링으로 넘깁니다. 새로 나타난 작은(≤64KB) 세션은 첫 줄부터, 앱 시작 시 이미 활성인 세션은 끝에서 합류합니다.
- **탐색 주기**: 활성 파일 tail은 150ms, 새 세션 탐색(디렉토리 순회)은 1초 1회(`RESCAN_INTERVAL`). 매 폴링마다 전체 순회하는 코드로 되돌리지 마십시오.
- **도구 판정**: 경로의 홈 폴더 컴포넌트(`.claude`, `.codex`, `.gemini`)를 우선합니다. 경로로 도구가 확정되면 해당 어댑터만 사용합니다(무시 판단 존중).
- **테스트**: 어댑터나 테일링을 바꾸면 `src-tauri/src/session_watcher_tests.rs`와 `tests/fixtures/agents/`를 함께 갱신하고 `cargo test`를 통과시켜야 합니다.
- **이중 파일 핑퐁 방지**: Antigravity의 `transcript_full.jsonl`은 파일 크기가 커서 오프셋 충돌을 유발하므로 **반드시 무시**하고 `transcript.jsonl`만 고정 감시해야 합니다.
- **7가지 AgentState**:
  - `idle`: 대기 상태
  - `thinking`: 생각/분석 중
  - `busy`: 도구(Tool)/명령어 실행 중
  - `talking`: 작업 완료/성공 답변 출력
  - `error`: 실패 또는 에러 발생
  - `click_react`: 사용자가 캐릭터를 클릭했을 때의 반응
  - `sleeping`: 5분(300초) 이상 미활동 시 수면


### ③ 상태 큐잉 (State Queueing) & 최소 유지 시간
- 짧은 명령어나 빠른 질문의 경우 `thinking` 상태가 0.1초 만에 `talking`으로 덮어써지는 문제를 방지하기 위해, `thinking` 발생 시 **최소 1.5초간 생각 모션을 보장**한 후 다음 상태(`talking` 등)로 전환하는 큐 스케줄링이 구현되어 있습니다.

### ④ 마우스 휠 크기 조절 & 캐릭터 내부 스케일링
- 창 크기를 동적으로 변경할 때 발생하는 우클릭 메뉴 잘림 및 DPI 왜곡을 방지하기 위해, 투명 윈도우는 640x920으로 넉넉하게 고정하고 휠 조절 시 **캔버스 내부에서 캐릭터 모델 배율(60% ~ 200%)만 부드럽게 확대/축소**됩니다.
- 캐릭터 크기 변화에 맞춰 마우스 클릭 감지 영역(`isOverCharacter`)도 정밀하게 자동 연동되어 투명 영역 클릭 관통이 100% 유지됩니다.

### ⑤ 스마트 폴백 (Smart Fallback)
- `characters.json`에 새로운 2D 도트나 단순 Live2D 캐릭터를 등록할 때 특정 상태(예: `busy`, `error`)가 정의되어 있지 않더라도, `resolveFallbackState()` 함수를 통해 `thinking` -> `talking` -> `idle` 순으로 자동 대체되어 절대 런타임 오류가 발생하지 않습니다.

### ⑥-1 데스크톱 상주 (`src-tauri/src/desktop.rs`)
- 메인 창은 `skipTaskbar: true`, `visible: false`로 생성 → 주 모니터 정중앙에 배치 후 `show()` (마지막 위치 기억은 사용자 결정으로 제거됨). 트레이 생성 실패 시 작업표시줄 표시로 자동 복귀.
- 트레이의 '항상 위'/'위치 잠금' 토글은 `tray-action` 이벤트로 오버레이(`main.js`)에 위임하고, 오버레이가 `sync_tray_state`로 체크 표시를 맞춥니다(상태의 단일 원천 = 오버레이 localStorage).
- `app_config.json`을 수정할 때는 반드시 `update_app_config()`를 사용하십시오(동시 쓰기 직렬화).
- 창 크기/위치는 `src-tauri/src/window_fit.rs`가 모니터 작업 영역에 맞춥니다. 사용자가 드래그한 뒤에는 `FitMode::SizeOnly`(위치는 절대 바꾸지 않고 창 크기만 맞춤), 앱 시작과 "화면 가운데로 불러오기"는 정중앙 배치(`KeepVisible`)입니다. 화면 가장자리에 걸치거나 숨기는 것은 의도된 사용법이므로 드래그 후 위치를 자동으로 되돌리는 규칙을 추가하지 마십시오. 640x920은 "최대(설계) 크기"이지 고정 크기가 아닙니다. 프론트엔드는 `window.innerWidth/innerHeight` 기준으로 캐릭터 크기를 계산해야 하며 920px을 가정하지 마십시오.

### ⑥-2 오프라인 런타임 (`src/lib/`)
- `live2d-loader.js`가 Core → Pixi → 플러그인 순서로 로컬 우선 로드하고 결과를 `window.__live2dReady`로 노출합니다. `index.html`에 CDN `<script>`를 다시 직접 넣지 마십시오.
- Live2D Cubism Core(독점 라이선스)는 레포에 커밋하지 않습니다(`.gitignore`).

### ⑥ GUI 캐릭터 매니저 및 멀티 윈도우 동기화
- `tauri.conf.json`에 `settings` 윈도우를 `"visible": false`로 사전 선언(Pre-warmed)하여 앱 기동 시 백그라운드에서 초기화를 완료해 둡니다.
- 우클릭 메뉴의 `open_settings_window`를 통해 0.001초 만에 `show()` 및 `set_focus()`로 팝업하며, 닫기(X) 시 `hide()`로 처리하여 빠른 재오픈을 보장합니다.
- 설정 창에서 캐릭터 변경/추가/삭제 시 Rust 백엔드가 `characters-updated` 및 `switch-character` 이벤트를 메인 창으로 브로드캐스트하여 실시간 양방향 동기화.

---

## 5. 캐릭터 추가 및 스키마 확장 가이드

- **폴더 기반 자체 완결형(Self-Contained) 아키텍처**:
  - 각 캐릭터는 자신의 폴더 안에 모델/이미지 에셋과 `character.json` 설정 파일을 함께 보관합니다.
  - 앱 기동 시 Rust 백엔드가 `src/characters/` 및 `src/characters/custom_characters/` 디렉토리를 자동 스캔하여 런타임에 병합합니다.
- **공개/기본 캐릭터**: `src/characters/<캐릭터명>/`에 배치합니다 (`mao`, `wanko`, `pixel_cat` 등 라이선스 안전 에셋).
- **개인 로컬 전용 커스텀 캐릭터 (Git 커밋 방지)**: `src/characters/custom_characters/<캐릭터명>/` 폴더에 에셋과 `character.json`을 함께 배치합니다. 이 디렉토리는 `.gitignore`로 보호되어 원격 저장소에 업로드되지 않습니다.
- **최상단 `src/characters.json`**: 신규 사용자를 위한 공개 참조용 스키마 템플릿(가이드) 파일입니다.
- **런타임 앱 설정**: `src/app_config.json`에 `activeCharacter` 및 전역 말풍선 설정이 저장되며, `.gitignore`로 보호됩니다.
- 예시 (`src/characters/custom_characters/my_bot/character.json`):
  ```json
  {
    "name": "나만의 도트 봇",
    "type": "image",
    "states": {
      "idle": { "src": "characters/custom_characters/my_bot/idle.png", "subtitles": ["대기 중"] },
      "thinking": { "src": "characters/custom_characters/my_bot/work.gif", "subtitles": ["작업 중!"] },
      "talking": { "src": "characters/custom_characters/my_bot/done.png", "subtitles": ["완료!"] }
    }
  }
  ```

---

## 6. 변경 시 필수 체크리스트 & 행동 수칙

1. [ ] 백엔드 Rust 변경 시: `src-tauri`에서 `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` 통과 확인 (CI와 동일).
   - 프론트엔드 변경 시(선택): `node tests/smoke/frontend_smoke.cjs --label check` 로 오프라인 스모크 확인.
2. [ ] 프론트엔드 변경 시: 마우스 클릭 관통 및 투명 배경이 정상 유지되는지 확인.
3. [ ] **경로 표기 및 개인정보 보호 (중요)**: 마크다운(`README.md` 등) 및 코드 작성 시 개인 PC 절대 경로(`C:\Users\...`, `file:///...`)는 일절 사용을 금지하며, 반드시 **프로젝트 루트 기준의 상대 경로(Relative Path, 예: `src/...`)**만 작성할 것.
4. [ ] 새 파일/설정 추가 시: 민감한 API 키나 개인 정보가 하드코딩되지 않았는지 확인.
5. [ ] **Git 명령어 실행 원칙 (중요)**: `git add`, `git commit`, `git push`, `git reset` 등 Git 상태 변경이나 원격 저장소 푸시 명령어는 **AI 에이전트가 임의로 실행하지 않고, 반드시 사용자에게 먼저 확인을 받거나 사용자가 직접 실행하도록 명령어만 안내**할 것.
