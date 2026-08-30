# Claude Code 작업 지침 (CLAUDE.md)

이 문서는 **Claude Code CLI** 환경에서 ScreenMate 프로젝트를 수정하거나 확장할 때 사용하는 작업 지침서입니다.

---

## 📌 공통 가이드라인 참조 (Single Source of Truth)

- **프로젝트의 핵심 아키텍처, 빌드 규칙 및 불변 가이드라인은 [AGENTS.md](./AGENTS.md)를 최우선으로 참조합니다.**
- 본 파일은 Claude Code 환경에서 작업 시 필요한 보충 사항과 특이사항을 기록합니다.

---

## 💡 Claude Code 작업 환경 보충 사항

1. **세션 로그 감시 구조 (`src-tauri/src/session_watcher.rs`)**:
   - 감시 디렉토리: `~/.claude/projects/`
   - 세션 파일 형식: 대화별 `.jsonl` 파일
   - 가장 최근에 수정된 세션(mtime)을 150ms 주기로 초고속 추적하여 캐릭터 상태(`idle`, `thinking`, `busy`, `talking`, `error`)에 즉시 반영합니다.

2. **빌드, 문서 및 검증 가이드**:
   - 빌드/문법 검사: `cd src-tauri && cargo check`
   - 로컬 테스트 실행: `cd src-tauri && cargo run`
   - ⚠️ 주의: 마우스 클릭 관통(`cursor_watcher.rs` + `main.js`) 로직을 임의로 제거하거나 CSS `pointer-events: none`만으로 대체하지 마십시오.
   - ⚠️ **경로 표기 원칙 (불변)**: 마크다운(`README.md` 등) 및 코드에 개인 로컬 절대 경로(`file:///...`, `C:\Users\...`)는 절대 작성하지 않고, 반드시 **상대 경로(예: `src/...`)**만 사용합니다.

3. **Git 명령어 실행 원칙 (불변 규칙)**:
   - `git add`, `git commit`, `git push`, `git reset` 등 Git 상태를 변경하거나 원격 저장소에 영향을 주는 명령어는 **임의로 실행하지 않고, 반드시 사용자의 사전 확인을 거치거나 사용자가 직접 실행할 수 있도록 안내**해야 합니다.
