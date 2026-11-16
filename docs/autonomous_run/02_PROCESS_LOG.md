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

- **커밋 3** (`2e252a3`).

## Step 4+5. 트레이 상주 · 창 위치 기억/잠금 (ROADMAP P0) + 크기 슬라이더 · 효과음 (P1)

> 계획에서는 별도 커밋이었으나 우클릭 메뉴(`index.html`/`main.js`)를 함께 고치게 되어 **한 커밋으로 합침**.

- **결정 근거**: ROADMAP v0.2.0의 남은 P0 2개와 작은 P1 2개. 모두 "매일 켜두는 상주 앱의 기본기"에 해당하고 스코프 아웃 항목과 무관.
- **백엔드** (`src-tauri/src/desktop.rs` 신규, `lib.rs`, `Cargo.toml`, `tauri.conf.json`):
  - 트레이 아이콘 + 메뉴(숨기기/보이기, 캐릭터 매니저, 항상 위 ✓, 위치 잠금 ✓, 종료). 좌클릭 = 보이기/숨기기. `tauri` `tray-icon` 피처.
  - 메인 창 `skipTaskbar: true`. **트레이 생성 실패 시 작업표시줄 표시로 자동 복귀**(앱에 접근할 수단이 사라지지 않도록).
  - 창 위치: `Moved` 이벤트 → 0.8초 디바운스 후 `app_config.json`의 `windowPosition`에 저장, 종료 시 즉시 저장. 시작 시 **캐릭터가 서 있는 하단 중앙 지점이 연결된 모니터 안에 있을 때만** 복원(보조 모니터를 뺀 경우 화면 밖 복원 방지). 위치 이동 깜빡임을 막기 위해 메인 창은 숨김 상태로 생성 후 복원 → `show()`.
  - `update_app_config()` + 전역 Mutex: 설정 파일을 여러 곳(설정 창 명령, 위치 저장기)에서 동시에 읽고-고치고-쓰면 한쪽 변경이 사라질 수 있어 **읽기-수정-쓰기를 직렬화**하고 기존 변경 지점 6곳을 모두 이 함수로 교체 *(작업 중 판단한 예방 조치)*.
- **프론트엔드** (`src/main.js`, `src/index.html`, `src/style.css`, `src/sfx.js` 신규):
  - 우클릭 메뉴: 60–200% 1% 단위 슬라이더, 위치 잠금 토글, 효과음 On/Off + 볼륨, 트레이로 숨기기.
  - 위치 잠금 시 드래그해도 `startDragging`을 호출하지 않음(클릭 반응은 유지). 클릭 관통 로직은 수정하지 않음.
  - 트레이 ↔ 오버레이 동기화: 트레이 토글은 `tray-action` 이벤트로 오버레이에 위임, 오버레이가 `sync_tray_state`로 체크 표시를 맞춤(상태의 단일 원천 = 오버레이).
  - 효과음: WebAudio로 합성(완료=상승 3화음, 에러=하강 2음, 클릭=팝), 에셋 파일 0개, **기본 OFF**, 같은 소리 0.7초 내 중복 방지.
  - **기존 버그 발견·수정**: '항상 위' 설정을 OFF로 저장해도 재시작 시 창에 적용되지 않았음(UI만 OFF 표시). 시작 시 `set_always_on_top`을 호출하도록 수정.
- **검증**:
  - `cargo clippy --all-targets` 경고 0, `cargo test` 26 통과(창 위치 판정 테스트 5개 추가: 보조 모니터 분리, 음수 좌표 모니터, 창 상단이 화면 밖인 경우 등), `cargo build`로 Linux 링크 성공.
  - 스모크 상호작용 검사: 우클릭 메뉴 열림, 슬라이더 150% → 저장값 `1.5`, 잠금 OFF 드래그 시 `startDragging` 1회 / **잠금 ON 시 0회**, 트레이 이벤트로 잠금 해제됨, 효과음 켠 뒤 `talking` 전환 시 재생 호출 `true`.
  - **스모크로 잡은 버그**: 효과음을 켤 때 나는 "미리 듣기" 소리가 중복 방지 타이머를 소모해, 직후의 실제 완료 알림음이 무시됨 → 미리 듣기는 타이머에 기록하지 않도록 수정 후 재검증 통과.
- **한계(미검증)**: 실제 트레이 아이콘 표시/메뉴 클릭, 다중 모니터 복원, WebView2의 오디오 자동재생 정책(첫 클릭 전 무음일 가능성)은 Windows 실기기에서만 확인 가능 → 체크리스트.

- **커밋 4** (`c2c3374`).

## Step 6. CI 워크플로

- **변경**: `.github/workflows/ci.yml` — Rust 작업(`cargo check --locked`, `cargo clippy --all-targets -D warnings`, `cargo test`) + 프론트 작업(JS 문법 검사, Playwright 오프라인 스모크: 페이지 에러 0, 캐릭터 표시, 상태 반응 3회 이상, 위치 잠금 시 드래그 0회를 **실패 조건**으로 검사, 스크린샷 아티팩트 업로드).
- **검증**: 동일 명령을 컨테이너에서 실행해 모두 통과. 단, GitHub Actions 위에서의 실제 실행은 푸시가 막혀 확인하지 못함.
- **판단**: clippy는 `-D warnings`로 엄격하게 설정(현재 경고 0이므로 앞으로의 회귀를 막는 용도).

## Step 7. AFTER 측정 & 문서

- BEFORE와 같은 명령으로 재측정 → [`03_AFTER.md`](./03_AFTER.md). 스모크는 3개 캐릭터 × (Core 없음 / Core 로컬) 두 조건.
- 검증용으로 임시 배치했던 비공식 Cubism Core 사본은 측정 후 **삭제**(gitignore 대상이라 커밋되지 않음).
- 프로젝트 문서 갱신: `README.md`(새 기능·구조·라이선스), `ROADMAP.md`(완료 체크 4건 + 부분 완료 표기), `PROJECT_HISTORY.md`(17번 항목), `AGENTS.md`(테일링/탐색/트레이/오프라인 규칙 — 다음 에이전트가 되돌리지 않도록).
- [`04_MANUAL_TEST_CHECKLIST.md`](./04_MANUAL_TEST_CHECKLIST.md): 사람이 Windows에서 확인할 항목 정리.

## 일부러 하지 않은 것

- **PR 생성 / main 병합**: 요청받지 않았고, 사람 검토 없이 main에 들어가면 안 된다고 판단.
- **자동 시작(P1, `tauri-plugin-autostart`)**: 새 플러그인 의존성 + OS 레지스트리 변경이 필요하고, 실기기 검증 없이 넣기엔 되돌리기 비용이 큼 → 보류.
- **2D 모션 강도/GIF 모드(P2), v0.3 이후 항목**: 우선순위가 낮고 이번 범위(검증 가능한 개선)를 넘어섬.
- **기존 코드 전체 `cargo fmt`**: 원래 포맷되지 않은 파일을 일괄 포맷하면 diff가 불어나 검토가 어려워짐 → 새로 만든 파일만 포맷.
- **2D 냥이의 화면 중간 배치**: 원래 동작이며 의도인지 불명확해 손대지 않음(관찰만 기록).

## Step 8. 마무리 커밋 & 푸시

- **커밋 5** (`2824b9a`): CI + AFTER 측정 + 문서.
- **푸시**: 아래 "최종 상태" 참고.

## 최종 상태

- 브랜치 `claude/agent-autonomous-development-w0d192`에 기준 커밋 `a98bebd` 이후 **6개 커밋**(이 기록 갱신 포함).
- **원격 푸시 실패(2회 시도)**: `403 — Claude doesn't have GitHub access to chang406/ScreenMate`. 네트워크 문제가 아니라 권한 문제여서 재시도로 해결되지 않음.
  - 해결 방법: claude.ai에서 GitHub 연결을 다시 하거나 저장소에 Claude GitHub App을 설치한 뒤 다시 푸시.
  - 대안: 세션에서 함께 전달한 `git bundle` 파일로 로컬에 그대로 가져올 수 있음 → `git fetch <bundle 파일> claude/agent-autonomous-development-w0d192:claude/agent-autonomous-development-w0d192`
- main 브랜치, 원격 저장소, PR에는 **아무 변경도 하지 않음**.
