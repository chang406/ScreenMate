# 🤖 자율 개발 실험 기록 (Autonomous Run Record)

> **실험 질문**: "AI 코딩 에이전트에게 지시·검토·테스트 없이 레포를 맡기면 어디까지 발전시키는가?"
>
> 이 폴더는 그 실험의 **검증용 증거 묶음**입니다. 작업 **이전(BEFORE) → 과정(PROCESS) → 이후(AFTER)** 를
> 동일한 측정 명령으로 비교할 수 있게 남겨 두었습니다.

| 항목 | 값 |
| :--- | :--- |
| 작업 브랜치 | `claude/agent-autonomous-development-w0d192` (main 아님) |
| 시작 기준 커밋 | `a98bebd` (feat(v0.2.0): 다중 에이전트 어댑터 …) |
| 수행 주체 | Claude Code (클라우드 컨테이너, Linux 헤드리스) |
| 사람의 개입 | 최초 요청 1회 + "기록 문서도 만들어 달라" 추가 요청 1회. 그 외 검토/테스트 없음 |
| 판단 기준 | 레포 자체의 [`AGENTS.md`](../../AGENTS.md)(철학·불변 규칙), [`ROADMAP.md`](../../ROADMAP.md)(미완 P0/P1) |

---

## 📂 문서 구성 (읽는 순서)

1. [`01_BEFORE.md`](./01_BEFORE.md) — 손대기 전 상태: 빌드/테스트/정적분석 수치, 발견한 결함과 재현 증거, 스크린샷
2. [`02_PROCESS_LOG.md`](./02_PROCESS_LOG.md) — 단계별 타임라인: 무엇을, **왜** 골랐는지, 실행 명령과 결과, 실패/포기한 것
3. [`03_AFTER.md`](./03_AFTER.md) — 같은 측정의 재실행 결과와 BEFORE 대비 비교표
4. [`04_MANUAL_TEST_CHECKLIST.md`](./04_MANUAL_TEST_CHECKLIST.md) — 컨테이너에서 검증할 수 없었던 항목(Windows 실기기)을 사람이 직접 확인하는 체크리스트

보조 자료:
- `raw/` — 측정 명령의 원본 출력(가공 전). `before_*` / `after_*` 쌍으로 비교 가능
- `screens/` — 오프라인 스모크 테스트 스크린샷 `before_*.png` / `after_*.png`

---

## 🔁 직접 재현/비교하는 법

```bash
# 1) 무엇이 바뀌었나
git log --oneline a98bebd..claude/agent-autonomous-development-w0d192
git diff --stat a98bebd..claude/agent-autonomous-development-w0d192

# 2) 백엔드 측정 (src-tauri 에서)
cargo check
cargo test                                  # 단위 테스트
cargo test -- --ignored --nocapture bench   # 세션 탐색 비용 벤치마크
cargo clippy

# 3) 프론트엔드 오프라인 스모크 (프로젝트 루트에서, playwright 필요)
node tests/smoke/frontend_smoke.cjs --label mycheck --out docs/autonomous_run/screens
```

> 💡 BEFORE 수치를 직접 다시 뽑고 싶다면 `git checkout a98bebd` 후, 이 브랜치의
> `src-tauri/src/session_watcher_tests.rs`, `tests/fixtures/agents/*`, `tests/smoke/` 를 가져와 같은 명령을 실행하면 됩니다.
> (원본 코드에 테스트 파일을 붙이는 방법은 `01_BEFORE.md` 참고)

## ⚖️ 기록 원칙

- 수치와 출력은 **실제로 실행한 결과만** 적습니다. 실행하지 못한 검증은 "미검증"이라고 명시합니다.
- 실패, 되돌린 시도, 일부러 하지 않은 것도 숨기지 않고 [`02_PROCESS_LOG.md`](./02_PROCESS_LOG.md)에 남깁니다.
- 개인 로컬 절대 경로는 기록하지 않습니다(레포 규칙).
