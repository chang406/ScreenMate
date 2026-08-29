# ScreenMate 🐾✨
> **초경량 데스크톱 AI 스크린 메이트 (Live2D & 2D 지원)**

**ScreenMate**는 **Antigravity (IDE / CLI)** 및 **Claude Code (CLI)**와 연동되어, 사용자의 코딩 및 질문 작업 상태를 실시간으로 감지하여 화면 위에 항상 떠 있는 귀여운 캐릭터(Live2D / 2D)로 반응해 주는 데스크톱 오버레이 파트너입니다.

---

## 🌟 주요 기능

### 1. 다중 AI 에이전트 실시간 감시 (Multi-Agent Auto Watcher)
- **지원 도구**: Antigravity IDE, Antigravity CLI (`agy`), Claude Code CLI
- **작동 원리**: 로컬 대화 로그(`transcript.jsonl`, `~/.claude/projects/`)를 150ms 초고속 tail 감시하여 가장 최근에 활성화된 세션에 자동 반응.
- **7가지 세부 상태(State) 지원**:
  1. `idle` (대기): 편안한 호흡 모션 및 잔잔한 대기 대사
  2. `thinking` (생각 중): 갸웃거리며 생각하는 집중 모션 (*"어디 보자... 생각 중!"*)
  3. `busy` (도구/명령어 실행 중): 마법봉 휘두르기/시전 액션 모션 (*"열심히 코딩 중이에요! 🪄"*)
  4. `talking` (작업 완료/성공): 빙글 회전 점프 환호 모션 (*"작업 완료! 다 만들었어요! ✨"*)
  5. `error` (에러/실패 감지): 깜짝 놀라며 물러서는 모션 (*"어라? 에러가 발생했어요! 💦"*)
  6. `click_react` (마우스 터치): 캐릭터를 가볍게 탭하면 윙크/손인사 응원 멘트 (*"화이팅이에요! 💕"*)
  7. `sleeping` (5분 미활동): 조는 모션 및 대사 (*"Zzz... (새근새근)"*)

### 2. Live2D 4K 고화질 & 2D 멀티 캐릭터 지원
- **Mao (Live2D 4K)**: Live2D Cubism 3/4 정식 프로 런타임 (4096x4096 고해상도 텍스처, 모션 7종, 표정 8종 완비)
- **냥이 (2D Cat)**: 상황별 6종 전용 SVG 일러스트 탑재
- **스마트 폴백 (Smart Fallback)**: 모션이 부족한 Live2D나 이미지가 1~3장뿐인 단순 도트 캐릭터를 넣어도 에러 없이 자동으로 상태를 대체하여 안전하게 구동

### 3. 방해 없는 미니멀 UI/UX & 마우스 제어
- **완벽한 투명 마우스 클릭 관통 (Per-pixel Click-Through)**: 캐릭터 몸체만 클릭/드래그되고, 투명 배경은 뒤쪽 창(VS Code, 웹 브라우저)으로 100% 통과
- **마우스 휠(Wheel) 크기 조절**: 캐릭터 위에서 휠을 굴려 70% ~ 150%로 자유롭게 확대/축소 (창 크기 동적 연동으로 윗부분 모자 잘림 0%)
- **마우스 우클릭 미니멀 메뉴**:
  - 🔍 크기 조절 (80% / 100% / 120% / 140%)
  - 📌 항상 위에 고정 (ON / OFF 토글 - OFF 시 다른 작업 창 뒤로 자연스럽게 내려감)
  - 🔄 캐릭터 전환 (Mao ↔ 냥이)
  - ❌ 프로그램 안전 종료

---

## 🚀 실행 방법

```bash
cd src-tauri
cargo run
```
*(또는 npm / cargo tauri dev)*

---

## 🎨 새로운 캐릭터 추가 방법 (`src/characters.json`)

`src/characters/` 폴더에 새 폴더를 만들고 이미지나 Live2D 모델을 넣은 뒤, `src/characters.json`에 등록하면 끝납니다!

```json
{
  "activeCharacter": "my_char",
  "characters": {
    "my_char": {
      "name": "나만의 캐릭터",
      "type": "image",
      "states": {
        "idle": { "src": "characters/my_char/idle.png", "subtitles": ["대기 중"] },
        "thinking": { "src": "characters/my_char/work.gif", "subtitles": ["작업 중!"] },
        "talking": { "src": "characters/my_char/done.png", "subtitles": ["완료!"] }
        // busy, error 등은 없으면 자동으로 thinking/talking/idle로 대체됨!
      }
    }
  }
}
```

---

## 📁 프로젝트 구조

```
ScreenMate/
├── README.md                      # 프로젝트 소개 및 사용 가이드
├── PROJECT_HISTORY.md             # 프로젝트 개발 히스토리 및 기술 맥락
├── Implementation Plan.md         # 상세 구현 계획서
├── src-tauri/                     # 백엔드 (Rust / Tauri v2)
│   ├── Cargo.toml
│   ├── tauri.conf.json            # 창 설정 (투명, 프레임리스, 항상위)
│   └── src/
│       ├── main.rs
│       ├── lib.rs                 # 클릭 관통, 창 리사이즈, 항상위 제어 커맨드
│       ├── cursor_watcher.rs      # 16ms OS 레벨 마우스 커서 감시
│       └── session_watcher.rs     # Antigravity 및 Claude Code 세션 실시간 감시
└── src/                           # 프론트엔드 (Webview)
    ├── index.html                 # 캔버스 / 이미지 / 말풍선 / 우클릭 메뉴
    ├── style.css                  # 다크 글래스모피즘 스타일 및 애니메이션
    ├── main.js                    # 렌더러, 휠 줌, 스마트 폴백, 인터랙션
    ├── characters.json            # 캐릭터 설정 및 대사/모션 매핑
    └── characters/                # 캐릭터 에셋
        ├── mao/                   # (4K Live2D Mao 모델 에셋 일체)
        └── pixel_cat/             # (2D 냥이 SVG 6종)
```
