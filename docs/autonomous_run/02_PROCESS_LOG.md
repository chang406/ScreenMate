# 02. PROCESS LOG — 자율 개발 과정 기록

> 각 단계는 **결정 → 근거 → 변경 → 실행/결과 → 커밋** 순서로 기록합니다.
> 커밋 해시는 해당 단계 커밋 직후 다음 단계에서 채워 넣습니다(자기 자신의 해시는 커밋 전에 알 수 없기 때문).

---

## Step 0. 탐색 & 계획 수립

- **한 일**: `AGENTS.md`, `CLAUDE.md`, `ROADMAP.md`, `src-tauri/src/*.rs`, `src/main.js`, `src/index.html`, 픽스처를 정독.
- **결정**: 새 기능을 "발명"하기보다 ① 레포가 이미 약속한 핵심 가치(에이전트 상태를 *정확하게*, *가볍게* 보여주기)의 결함을 먼저 고치고 ② ROADMAP v0.2.0의 미완 P0 → P1 순으로 진행.
- **근거**: 사람이 검토하지 않는 조건이므로, 레포 자체에 적힌 우선순위·철학을 유일한 판단 기준으로 삼는 것이 가장 예측 가능하고 되돌리기 쉬움.
- **Git 규칙 해석**: 레포 규칙은 git 상태 변경 전 사용자 확인을 요구. 사용자가 "검토 없이 임시 브랜치에서 진행"을 명시하고 계획을 승인했으므로 **이 브랜치 한정** commit/push만 수행. main 푸시·PR 생성·force push 없음.
- **사용자 추가 요청 반영**: "이전/이후/과정을 확인할 수 있는 기록 문서" → 이 `docs/autonomous_run/` 폴더를 코드 수정 *전에* 생성.

## Step 1. 빌드 환경 확보 & 베이스라인 측정

- **명령**: `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev` → `cargo check`(1분 15초, 경고 0) → `cargo clippy`(경고 3) → `cargo test`(테스트 0개).
- **측정 도구 작성** (코드 수정 전):
  - `src-tauri/src/session_watcher_tests.rs` — 어댑터 분류/테일링 특성 테스트 14개 + 벤치마크 1개.
  - `tests/fixtures/agents/claude/realistic_session.jsonl`, `tests/fixtures/agents/chatgpt_codex/rollout_session.jsonl` — 실제 도구 로그 형태를 본뜬 픽스처(개인 경로 없음, `~` 표기).
  - `tests/smoke/frontend_smoke.cjs` — Rust 없이 오버레이를 헤드리스로 띄우는 오프라인 스모크(Tauri API 목 주입, 외부 요청 전부 차단).
- **결과**: 7/14 실패, 오프라인 시 기본 캐릭터 미표시 등 → 상세는 [`01_BEFORE.md`](./01_BEFORE.md).
- **관찰**: 컨테이너의 네트워크 정책상 `cdn.jsdelivr.net`, `cubism.live2d.com` 접근이 실제로 막혀 있음(HTTP 000). 즉 "오프라인 사용자" 상황이 이 환경에서 자연 재현됨.
- **커밋 1** (`3b2dcc8`): 기록 문서 + 측정 도구 + 실패하는 테스트("red" 상태 그대로). 이 커밋을 체크아웃해 `cargo test`를 돌리면 BEFORE의 7건 실패를 그대로 재현할 수 있습니다.

## Step 2. 세션 감시 엔진 견고화 (D1~D5 수정)

- **결정 근거**: 앱의 존재 이유가 "에이전트 상태를 정확히 보여주기"이므로, 새 기능보다 분류 정확도와 이벤트 유실을 먼저 고침. 테스트가 이미 결함을 수치로 보여주고 있어 수정 효과를 객관적으로 검증 가능.
- **변경** (`src-tauri/src/session_watcher.rs`):
  - `Tail`: 바이트 단위로 읽고 **마지막 `\n`까지만 소비**, 나머지는 다음 폴링으로 이월(D1). `from_utf8_lossy`로 인코딩 경계 안전.
  - `Tail::switch_to(path, StartAt)`: 앱 시작 시점의 세션은 끝에서 합류, 이후 새로 나타난 **작은(≤64KB) 세션은 첫 줄부터** 읽음(D5). 큰 파일(이어하기 세션)은 끝에서 시작해 과거 기록 재생 방지.
  - 파일별 읽기 위치 기억(`known_positions`) → 두 세션을 번갈아 쓸 때도 사이에 쓰인 이벤트를 놓치지 않음 *(작업 중 추가 발견한 문제)*.
  - 디렉토리 탐색: 150ms마다 → **1초마다 1회**, 세션 개수 집계와 최신 파일 탐색을 **한 번의 순회로 통합**(`scan_watch_roots`). 활성 파일 tail은 계속 150ms(반응 속도 동일)(D2).
  - `ClaudeAdapter`: `tool_result`가 든 user 레코드 → `busy`, `isApiErrorMessage` → `error`, `isMeta`/`summary`/`file-history-snapshot` 무시, `system`+`level:error` → `error`, `[Request interrupted…]` → `idle`(D3).
  - `ChatGptCodexAdapter`: Codex rollout `response_item`/`event_msg` 언래핑, 주입된 `<environment_context>` 무시, `apply_patch`→`edit`, `shell`→`command`(D4).
  - `AgentRegistry`: 경로의 **도구 홈 폴더(`.claude`, `.codex`, `.gemini`) 컴포넌트**를 먼저 보고 소유 도구 판정. *(작업 중 추가 발견: Claude는 프로젝트 폴더명을 cwd로 짓기 때문에 `~/.claude/projects/-home-me-gemini-app/`이 기존 코드에서는 Antigravity로 오판됨.)* 경로로 도구가 확정되면 그 어댑터의 "무시" 판단을 존중(다른 어댑터로 재분류하지 않음).
  - clippy 경고 3건 해소(`#[derive(Default)]`, `&dyn` 반환, 카운터 제거).
- **테스트 추가**: 새 세션 첫 줄, 대용량 세션 끝 시작, 세션 교차 재개, 경로 오판, 인터럽트, Codex 주입 메시지 무시, 스캔 집계 → 총 21개.
- **결과**: `cargo test` **21 통과 / 0 실패**(BEFORE 7/14) · `cargo clippy --all-targets` 경고 **0** · 벤치 `2.14ms/스캔 → 약 2.1ms CPU/s` (같은 머신에서 구 방식 환산 14.2ms CPU/s, BEFORE 측정 19.9ms CPU/s).
- **주의/한계**: 실제 Claude Code·Codex 로그는 버전에 따라 필드가 바뀔 수 있음. 픽스처는 공개적으로 알려진 형태를 본떠 손으로 작성한 것이며, 실제 사용자 로그로 검증하지는 못함(→ 체크리스트에 포함).

- **커밋 2** (`f75359c`).
- **푸시 시도 → 실패**: `git push` 가 `403 (Claude doesn't have GitHub access to chang406/ScreenMate)` 로 거부됨. 이 세션의 GitHub 앱 권한 문제로, 코드로 해결할 수 없음 → 로컬 커밋은 계속 쌓고 마지막에 재시도하기로 결정.

## Step 3. 오프라인 런타임 번들링 & Live2D 우아한 대체 (D6, ROADMAP P0)

- **결정 근거**: BEFORE에서 기본 캐릭터가 오프라인일 때 *완전히 사라지는* 것이 확인됨. ROADMAP P0이기도 함.
- **라이선스 판단**: Pixi.js / pixi-live2d-display는 MIT → 레포에 포함(LICENSE 동봉). **Cubism Core는 Live2D 독점 라이선스 → 레포에 넣지 않음**(npm에 비공식 재배포본이 있었지만 사용하지 않음). 대신 "로컬 파일 있으면 로컬, 없으면 CDN" 로더 + `.gitignore`.
- **변경**:
  - `src/lib/vendor/` — `npm pack`으로 받은 원본 파일 그대로 복사(SHA-256 기록: `src/lib/README.md`).
  - `src/lib/live2d-loader.js` — Core → Pixi → 플러그인 순서 보장(플러그인은 Core가 없으면 로드 시점에 예외를 던지므로), 원격 6초 타임아웃, 결과를 `window.__live2dReady`로 노출.
  - `src/main.js` — Live2D 불가 시 **2D 캐릭터(냥이)로 임시 대체** + 이유를 말풍선으로 안내. `activeCharacter` 설정은 건드리지 않아 환경이 갖춰지면 자동 복귀. `PIXI.Application` 생성도 try로 보호.
- **검증 (오프라인 스모크, 외부 요청 전부 차단)**:
  - Core 없음: Mao → 냥이로 대체되어 표시, 상태 4종 모두 말풍선 반응. 차단 요청 3건 → 1건(Core CDN만).
  - Core 로컬 배치(검증용으로 npm 비공식 사본을 *gitignore된 위치에 임시로* 둠): **Wanko(Live2D)가 완전 오프라인으로 렌더링됨**, 외부 요청 0건.
  - **예상 못 한 발견**: 같은 조건에서 Mao는 `Unknown error`로 실패. 원인 조사 → Mao 모델 파일 헤더가 **moc3 v5(Cubism 5)** 이고 확보 가능한 Core는 2019년판이라 미지원. 코드 결함이 아니라 Core 버전 문제 → 에러 말풍선에 "Core가 지원하는 moc3 버전"을 함께 표시하도록 개선, README에 "최신 Core 필요" 명시.
- **한계**: 최신 공식 Core로 Mao가 오프라인 렌더링되는지는 이 컨테이너(공식 CDN 차단)에서 검증 불가 → 체크리스트에 포함.

<!-- 이후 단계는 진행하면서 아래에 추가 -->
