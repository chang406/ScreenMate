# Antigravity / Gemini 작업 지침 (GEMINI.md)

이 문서는 **Google Antigravity IDE 및 CLI (`agy`)** 환경에서 ScreenMate 프로젝트를 수정하거나 확장할 때 사용하는 작업 지침서입니다.

---

## 📌 공통 가이드라인 참조 (Single Source of Truth)

- **프로젝트의 핵심 아키텍처, 빌드 규칙 및 불변 가이드라인은 [AGENTS.md](./AGENTS.md)를 최우선으로 참조합니다.**
- 본 파일은 Antigravity / Gemini 환경에서 작업 시 필요한 보충 사항과 특이사항을 기록합니다.

---

## 💡 Antigravity 작업 환경 보충 사항

1. **세션 로그 감시 구조 (`src-tauri/src/session_watcher.rs`)**:
   - Antigravity IDE 로그 위치: `~/.gemini/antigravity-ide/brain/`
   - Antigravity CLI 로그 위치: `~/.gemini/agy/logs/` 및 `~/.antigravity/logs/`
   - ⚠️ `transcript_full.jsonl`은 대용량 파일 오프셋 충돌을 방지하기 위해 필터링되어 있으므로, **`transcript.jsonl`만 고정 추적**해야 합니다.

2. **코드 및 문서 작성 가이드**:
   - Rust 코드 변경 후: `cd src-tauri && cargo check`를 실행하여 컴파일 오류가 없는지 검증합니다.
   - 프론트엔드 변경 시: 마우스 투명 관통(`set_click_through`) 및 투명 윈도우 배경이 정상 작동하는지 확인합니다.
   - ⚠️ **경로 표기 원칙 (불변)**: 마크다운(`README.md` 등) 및 코드에 개인 로컬 절대 경로(`file:///...`, `C:\Users\...`)는 절대 작성하지 않고, 반드시 **상대 경로(예: `src/...`)**만 사용합니다.

3. **Git 명령어 실행 원칙 (불변 규칙)**:
   - `git add`, `git commit`, `git push`, `git reset` 등 Git 상태를 변경하거나 원격 저장소에 영향을 주는 명령어는 **임의로 실행하지 않고, 반드시 사용자의 사전 확인을 거치거나 사용자가 직접 실행할 수 있도록 안내**해야 합니다.
