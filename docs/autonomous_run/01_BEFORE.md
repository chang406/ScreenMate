# 01. BEFORE — 작업 전 베이스라인

> 측정 시점: 기준 커밋 `a98bebd`, 코드 수정 **이전**
> 측정 환경: Linux 헤드리스 컨테이너 (Rust 1.97 / Node 22 / Playwright Chromium), Tauri Linux 의존성(webkit2gtk-4.1 등) 설치 후

---

## 1. 수치 요약

| 지표 | 결과 | 원본 |
| :--- | :--- | :--- |
| `cargo check` | ✅ 통과, 경고 0 | — |
| `cargo clippy` | ⚠️ 경고 **3건** (`derivable_impls`, `borrowed_box`, `manual_is_multiple_of`) | [`raw/before_clippy.txt`](./raw/before_clippy.txt) |
| 기존 자동화 테스트 | **0개** (레포에 테스트 없음, CI 없음) | — |
| 신규 작성한 특성(characterization) 테스트를 **원본 코드에** 실행 | **7 통과 / 7 실패** (+1 벤치마크) | [`raw/before_cargo_test.txt`](./raw/before_cargo_test.txt) |
| 세션 파일 탐색 비용 (2,000개 `.jsonl` 트리) | 전체 스캔 1회 ≈ **2.98ms**, 150ms마다 수행 → **약 19.9ms CPU/초** 를 탐색에만 사용 | [`raw/before_bench.txt`](./raw/before_bench.txt) |
| 오프라인 스모크: 2D 캐릭터(냥이) | ✅ 표시·상태 반응 정상 | [`raw/before_smoke.json`](./raw/before_smoke.json) |
| 오프라인 스모크: **기본 캐릭터 Mao(Live2D)** | ❌ `Init failed: ReferenceError: PIXI is not defined` → **캐릭터 미표시, 모든 상태 이벤트 무반응** | 같은 파일, `screens/before_mao.png` |

> 특성 테스트란? "이 코드가 *올바르다면* 이렇게 동작해야 한다"를 먼저 테스트로 적고, 원본 코드에 돌려서 현재 결함을 수치로 드러내는 방식입니다.
> 테스트 파일: `src-tauri/src/session_watcher_tests.rs` (원본에 붙일 때는 `session_watcher.rs` 끝에
> `#[cfg(test)] #[path = "session_watcher_tests.rs"] mod tests;` 3줄만 추가하면 됩니다.)

---

## 2. 발견한 결함 (증거 포함)

### D1. 기록 중인 "반쪽 라인"을 버려 이벤트가 유실됨 — `session_watcher.rs` `Tail::read_new_records`
- 원인: 파일 끝까지 `read_to_string` 후 위치를 전부 전진시킴. AI 도구가 한 줄을 두 번에 나눠 flush하면 앞/뒤 조각이 각각 JSON 파싱 실패로 버려짐.
- 증거: `tail_does_not_drop_partially_written_line` **실패** — 기대 `[{"type":"user","message":"hi"}]`, 실제 `[]`.
- 영향: 사용자의 입력이나 완료 메시지가 가끔 반응 없이 지나감(재현이 어려운 "가끔 안 움직여요" 버그 유형).

### D2. 150ms마다 감시 폴더 전체를 재귀 탐색 — `find_latest_session_file`
- 원인: 최신 세션 파일을 찾기 위해 매 폴링마다 모든 하위 폴더를 `read_dir` + `metadata`.
- 증거: 벤치마크 2,000파일 기준 **≈19.9ms CPU/초** (장기 사용자의 `~/.claude/projects`는 수천 개 파일이 흔함). "초경량 상주" 철학([`AGENTS.md`](../../AGENTS.md))과 충돌.

### D3. Claude Code 실제 로그 포맷 오분류 — `ClaudeAdapter`
- 원인: Claude Code는 도구 실행 결과를 `type: "user"` + `tool_result` 블록으로 기록하는데, 모든 `user` 레코드를 `thinking`으로 처리.
- 증거:
  - `claude_tool_result_inside_user_record_is_busy` **실패** — 실제 `Thinking`, 기대 `Busy(general)`.
  - `claude_realistic_fixture_sequence` **실패** — 도구 호출마다 `busy → thinking → busy → thinking …` 으로 **깜빡임**.
    프론트엔드는 `thinking` 진입 시 1.5초 잠금을 걸기 때문에, 실제 화면에서는 도구를 쓸 때마다 "생각 중" 대사·모션이 끼어듦.
  - `claude_api_error_message_is_error` **실패** — API 오류(`isApiErrorMessage`)가 `Talking`(작업 완료!)으로 표시됨.
  - `claude_meta_records_are_ignored` **실패** — `/clear` 같은 메타 레코드(`isMeta`)가 `Thinking`을 유발.
- 참고: 기존 샘플 픽스처(`claude/sample_session.jsonl`)는 실제 포맷과 달라 이 문제를 가리고 있었음(통과).

### D4. OpenAI Codex CLI 실제 로그 포맷 미인식 — `ChatGptCodexAdapter`
- 원인: 현 Codex CLI는 `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`에 `{"type":"response_item","payload":{…}}` / `{"type":"event_msg",…}` 래퍼로 기록. 어댑터는 최상위 `role` 필드만 확인.
- 증거: `codex_rollout_fixture_sequence` **실패** — 기대 7개 상태, 실제 **0개**(완전 무반응). `codex_rollout_records` 실패.
- 추가: `apply_patch`(Codex의 파일 수정 도구)는 `tool_category`에서 `edit`이 아닌 `general`로 분류됨.

### D5. 새 세션 첫 줄 누락 가능성 (코드 리딩)
- `Tail::switch_to`는 새로 감지한 파일을 항상 **파일 끝**부터 읽음. 새 대화를 시작하면 첫 줄(사용자 입력)이 이미 기록된 뒤 감지되므로 첫 `thinking` 반응을 놓칠 수 있음.
- 원본 API로는 "처음부터 읽기" 자체가 불가능하여 테스트로 표현하지 못함 → AFTER에서 테스트 추가 예정.

### D6. 오프라인/폐쇄망에서 기본 캐릭터가 통째로 사라짐 — `src/index.html`, `src/main.js`
- 원인: Pixi.js / pixi-live2d-display / Cubism Core 를 CDN에서만 로드. 실패 시 `loadLive2D()`의 `new PIXI.Application`이 try 밖에서 예외 → `init()` 전체 중단.
- 증거(오프라인 스모크, 외부 요청 전부 차단):
  - 차단된 요청 3건: `cubism.live2d.com/…/live2dcubismcore.min.js`, `cdn.jsdelivr.net/npm/pixi.js@6.5.9/…`, `cdn.jsdelivr.net/npm/pixi-live2d-display@0.4.0/…`
  - Mao: 콘솔 `Init failed: ReferenceError: PIXI is not defined`, 상태 이벤트 4종 전송 후에도 말풍선 텍스트 변화 없음(`"대기 중이에요"` 고정).
  - ROADMAP의 P0 "완전한 오프라인 정적 번들링"이 미완인 상태와 일치.

### D7. ROADMAP v0.2.0 P0 미완 항목
- 시스템 트레이 상주 / 창 위치 기억·복원 / 위치 잠금 — 코드에 구현 흔적 없음(`tray`, `Moved`, `windowPosition` 검색 결과 0건).

---

## 3. 스크린샷 (오프라인 스모크, 배경은 확인용 회색)

| 냥이 (2D) | Mao (Live2D, 기본 캐릭터) |
| :---: | :---: |
| ![before pixel_cat](./screens/before_pixel_cat.png) | ![before mao](./screens/before_mao.png) |
| 정상 표시 | **아무것도 렌더링되지 않음** |

---

## 4. 이번 실험에서 손대지 않기로 한 것 (스코프 판단)

- 챗봇/TTS/생산성 위젯/CPU 폴링 — `AGENTS.md`의 명시적 스코프 아웃.
- 클릭 관통 로직(`cursor_watcher.rs` + `main.js isOverCharacter/setIgnoringCursor`) — 불변 규칙. 수정하지 않음.
- Live2D Cubism Core 파일의 레포 포함 — 독점 라이선스(Live2D Proprietary Software License)라 커밋하지 않음. 대신 "로컬 파일 우선 + CDN 폴백" 로더로 해결 예정.
