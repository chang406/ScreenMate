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
- **커밋 1**: 기록 문서 + 측정 도구 + 실패하는 테스트("red" 상태 그대로). 이 커밋을 체크아웃해 `cargo test`를 돌리면 BEFORE의 7건 실패를 그대로 재현할 수 있습니다.

<!-- 이후 단계는 진행하면서 아래에 추가 -->
