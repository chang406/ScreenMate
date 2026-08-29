# ScreenMate 개발 히스토리 및 작업 맥락 (Project Context & History) 🐾

이 문서는 이전 프로젝트(`LiveCLI`, `Live-mate`)에서 시작하여 현재의 완성형 단일 프로젝트 **`ScreenMate`**로 발전해 온 전 과정과 핵심 아키텍처 결정 사항을 기록한 히스토리 문서입니다.

---

## 1. 프로젝트 발전 계보 (Timeline)

### 1단계: `LiveCLI` (Python 프로토타입)
- **스택**: Python (FastAPI + Uvicorn) + pywebview + VOICEVOX (음성 TTS) + Live2D (PIXI.js)
- **성취**: Live2D Mao 모델이 화면에 뜨고 Claude 세션 로그를 읽어 모션과 음성을 재생하는 기능 검증.
- **한계점**: 파이썬 가상환경(venv 수백 MB), Uvicorn 서버, pywebview, VOICEVOX 등 프로세스가 너무 많고 무거우며, 클릭 관통(Click-Through)과 창 최소화 시 Windows 버그가 발생함.

### 2단계: `Live-mate` (Rust / Tauri v2 아키텍처 전환)
- **스택**: Rust (Tauri v2) + Webview2
- **성취**: 별도 서버 프로세스 없는 초경량 단일 바이너리 구축, OS 레벨 커서 폴링(16ms) 기반의 완벽한 투명 마우스 클릭 관통(Click-through) 구현.
- **한계점**: 기본 뼈대만 만들고 프론트엔드가 단순 CSS 동그라미 얼굴(임시 플레이스홀더)에 머물러 있어 시각적 완성도가 부족했음.

### 3단계: `ScreenMate` (최종 통합 완성형 프로젝트)
- **방향**: `Live-mate`의 견고하고 가벼운 Tauri v2 백엔드에, `LiveCLI`의 풍부한 Live2D 렌더러 및 최신 `mao_ko` 4K 모델을 흡수 통합하여 **`ScreenMate`** 새 프로젝트로 완전 독립 구축.

---

## 2. 주요 개선 및 문제 해결 이력 (Key Milestones & Bug Fixes)

### 1) Antigravity IDE & CLI 실시간 세션 감시 연동
- Claude Code(`~/.claude/projects/`)뿐만 아니라, **Antigravity IDE(`~/.gemini/antigravity-ide/brain/`) 및 CLI(`agy` logs)**의 `transcript.jsonl` 로그를 150ms 주기로 초고속 감시하도록 확장.

### 2) 이중 파일 핑퐁 스위칭 버그 해결 (말풍선/상태 누락 이슈)
- **원인**: Antigravity 폴더에 `transcript.jsonl`과 `transcript_full.jsonl` 두 파일이 동시에 존재하여, 백엔드가 매 틱마다 대상 파일을 스위칭하면서 파일 읽기 오프셋이 초기화되어 새 로그를 버리는 문제 발생.
- **해결**: `transcript_full.jsonl`을 명시적으로 감시 대상에서 제외하고 대화 전용 `transcript.jsonl`만 고정 추적하도록 수정.

### 3) 짧은 질문/답변 시 상태 큐잉(State Queue) 및 최소 유지 시간 도입
- **원인**: 1초 만에 끝나는 짧은 질문에서 `Thinking`이 뜨자마자 0.001초 만에 `Talking`으로 덮어써져 생각하는 모습이 안 보이던 현상.
- **해결**: 프론트엔드에 상태 큐(Queue)를 적용하여 `Thinking`이 발생하면 **최소 1.5초간 생각 모션을 보장**한 뒤 부드럽게 `Talking`으로 전환되도록 시퀀스 스케줄링.

### 4) 마우스 휠(Wheel) 크기 조절 & 상단 윗부분 모자 잘림 해결
- **원인**: 고정된 윈도우 크기(500px)에서 캐릭터만 스케일업되어 130% 이상일 때 마녀 모자가 잘리는 현상.
- **해결**: 
  1. 배율(70% ~ 150%)에 맞춰 **Tauri 윈도우 창 크기(`set_window_size`)도 실시간으로 함께 확장**되도록 연동.
  2. Live2D 모델 캔버스 피팅 시 상단 28% 안전 마진을 확보하여 150% 최대 확대 시에도 단 1픽셀도 잘리지 않음.
  3. 설정 크기는 `localStorage`에 자동 저장되어 재실행 시에도 유지.

### 5) 단축키 충돌 방지 & 마우스 우클릭 미니멀 메뉴 도입
- 키보드 타이핑 시 단축키 충돌을 원천 차단하기 위해, 캐릭터 위 **마우스 우클릭 시 반투명 다크 팝업 메뉴** 지원:
  - 크기 프리셋 (80% / 100% / 120% / 140%)
  - 항상 위 고정 ON/OFF (OFF 시 백그라운드로 내려감)
  - 캐릭터 전환 (Mao ↔ 냥이)
  - 프로그램 종료

### 6) 4K 초고화질 `mao_ko` 최신 런타임 & 7가지 모드 전용 맵핑
- **4K 텍스처 (4096x4096)** 및 2025 최신 런타임 탑재.
- **7가지 모드 완비**: `idle`, `thinking`, `busy`(마법봉 코딩), `talking`(점프 환호), `error`(놀람/당황), `click_react`(탭 시 윙크/손인사), `sleeping`(5분 미활동 졸기).
- **스마트 폴백 (Smart Fallback)**: 새로운 도트나 단순 Live2D를 추가해도 누락된 모드는 자동으로 기본 상태로 대체되어 절대 에러가 나지 않는 구조 구현.

---

## 3. 개발 및 확장 가이드 (For Next Session)

- **빌드 및 실행**: `cd src-tauri && cargo run`
