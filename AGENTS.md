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

## 2. 빌드 및 실행 명령어 (Commands)

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

## 3. 핵심 아키텍처 및 불변 규칙 (Core Architecture & Rules)

### ① 투명 마우스 클릭 관통 (Per-pixel Click-Through)
- **백엔드 (`src-tauri/src/cursor_watcher.rs`)**: 16ms 주기로 OS 마우스 커서 좌표를 감시하여 프론트엔드로 `cursor-position` 이벤트를 전송.
- **프론트엔드 (`src/main.js`)**:
  - 캔버스/이미지의 알파 채널(투명도)을 실시간 검사.
  - 마우스가 캐릭터 불투명 영역 위에 있으면 `invoke('set_click_through', { ignore: false })` (클릭/드래그 가능).
  - 마우스가 투명 영역이나 빈 공간에 있으면 `invoke('set_click_through', { ignore: true })` (마우스가 뒤의 작업 창으로 통과).
- ⚠️ **주의**: 클릭 관통 로직을 무력화하거나 CSS `pointer-events: none`만으로 대체하지 마십시오 (OS 레벨 클릭 관통이 깨집니다).

### ② 세션 로그 감시 및 다중 에이전트 지원 (`session_watcher.rs`)
- **감시 대상**:
  - `~/.claude/projects/` (Claude Code)
  - `~/.gemini/antigravity-ide/brain/` (Antigravity IDE)
  - `~/.gemini/agy/logs/` & `~/.antigravity/logs/` (Antigravity CLI)
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

### ④ 마우스 휠 크기 조절 & 동적 윈도우 리사이즈
- 캐릭터 확대 시(70% ~ 150%) 상단 마녀 모자나 장식물이 잘리지 않도록 배율에 맞춰 Tauri 윈도우 크기도 `invoke('set_window_size', { width, height })`를 통해 동적으로 함께 조절됩니다.

### ⑤ 스마트 폴백 (Smart Fallback)
- `characters.json`에 새로운 2D 도트나 단순 Live2D 캐릭터를 등록할 때 특정 상태(예: `busy`, `error`)가 정의되어 있지 않더라도, `resolveFallbackState()` 함수를 통해 `thinking` -> `talking` -> `idle` 순으로 자동 대체되어 절대 런타임 오류가 발생하지 않습니다.

---

## 4. 캐릭터 추가 및 스키마 확장 가이드

- **공개/기본 캐릭터**: `src/characters/<캐릭터명>/`에 배치합니다 (Mao, 냥이 등 라이선스 안전 에셋).
- **개인 로컬 전용 커스텀 캐릭터 (Git 커밋 방지)**: `src/characters/custom_characters/<캐릭터명>/` 폴더에 배치합니다. 이 경로는 `.gitignore`로 보호되어 실수로 원격 저장소에 업로드되지 않습니다.
- `src/characters.json`의 `characters` 객체에 새 키를 추가합니다.
- 예시:
  ```json
  "my_bot": {
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

## 5. 변경 시 필수 체크리스트 & 행동 수칙

1. [ ] 백엔드 Rust 변경 시: `src-tauri`에서 `cargo check` 에러 0개 확인.
2. [ ] 프론트엔드 변경 시: 마우스 클릭 관통 및 투명 배경이 정상 유지되는지 확인.
3. [ ] **경로 표기 및 개인정보 보호 (중요)**: 마크다운(`README.md` 등) 및 코드 작성 시 개인 PC 절대 경로(`C:\Users\...`, `file:///...`)는 일절 사용을 금지하며, 반드시 **프로젝트 루트 기준의 상대 경로(Relative Path, 예: `src/...`)**만 작성할 것.
4. [ ] 새 파일/설정 추가 시: 민감한 API 키나 개인 정보가 하드코딩되지 않았는지 확인.
5. [ ] **Git 명령어 실행 원칙 (중요)**: `git add`, `git commit`, `git push`, `git reset` 등 Git 상태 변경이나 원격 저장소 푸시 명령어는 **AI 에이전트가 임의로 실행하지 않고, 반드시 사용자에게 먼저 확인을 받거나 사용자가 직접 실행하도록 명령어만 안내**할 것.
