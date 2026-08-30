# Live2D & 2D 이미지 멀티 캐릭터 스크린 메이트 (ScreenMate) 클린 구축 계획

`LiveCLI`와 `Live-mate` 두 프로젝트에서 불필요한 레거시 파일들을 모두 배제하고, **핵심 기능(초경량 Tauri 백엔드 + 손쉬운 캐릭터 교체가 가능한 프론트엔드)** 만 추출하여 깔끔하게 통합 구축합니다.

---

## 1. 아키텍처 및 핵심 설계

### ① 초경량 단일 백엔드 (Rust / Tauri v2)
- 복잡한 Python 가상환경, FastAPI 서버, 배치 스크립트 완전 제거
- **세션 로그 감시 (`session_watcher.rs`)**: Claude Code의 대화 로그(`.jsonl`)를 실시간 tail하여 `idle`, `thinking`, `talking` 판별
- **OS 커서 감시 (`cursor_watcher.rs`)**: 16ms 주기로 마우스 위치를 감시하여 캐릭터 몸체만 클릭/드래그되고 투명 영역은 마우스가 뒤로 통과(Click-through)되도록 제어

### ② 플러그형 멀티 캐릭터 렌더러 (Live2D & 2D 이미지/GIF 지원)
- **공통 캐릭터 매니저**: 상태 이벤트(`idle`/`thinking`/`talking`)를 받아 현재 활성화된 캐릭터의 렌더러로 전달
- **Live2D 렌더러**: PIXI.js + pixi-live2d-display 기반 (모션 `motion`, 표정 `expression` 제어)
- **2D 이미지/GIF 렌더러**: 상태별 이미지(PNG, WebP, GIF 등) 전환 제어
- **간편한 캐릭터 교체 UX**:
  - `characters.json` 설정 파일에서 캐릭터 경로/타입 지정
  - 앱 실행 중 **단축키 (예: `Tab` 또는 `C` 키)** 로 등록된 캐릭터들을 실시간으로 즉시 전환

---

## 2. 파일 및 폴더 구조 (불필요한 레거시 제외)

```
[ScreenMate / Live-mate]
├── src-tauri/                     # 백엔드 (Tauri v2 / Rust)
│   ├── Cargo.toml
│   ├── tauri.conf.json            # 투명/프레임리스/항상위 창 설정
│   └── src/
│       ├── main.rs
│       ├── lib.rs                 # 클릭 관통(set_click_through) 커맨드
│       ├── cursor_watcher.rs      # OS 커서 감시
│       └── session_watcher.rs     # Claude 세션 로그 감시
│
├── src/                           # 프론트엔드 (Webview)
│   ├── index.html                 # 캔버스 & 이미지 컨테이너, 말풍선 UI
│   ├── style.css                  # 투명 뷰포트 및 말풍선 스타일
│   ├── main.js                    # 캐릭터 매니저, 이벤트 브릿지, 클릭 관통 연동
│   ├── characters.json            # 캐릭터 등록 및 반응 매핑 설정
│   ├── libs/                      # 오프라인 구동용 Live2D/PIXI 라이브러리
│   └── characters/                # 캐릭터 에셋 폴더
│       ├── mao/                   # (Live2D 캐릭터 예시)
│       │   ├── Mao.model3.json
│       │   ├── Mao.moc3
│       │   ├── motions/
│       │   └── expressions/
│       └── pixel_cat/             # (2D 이미지/GIF 캐릭터 예시)
│           ├── idle.png (또는 gif)
│           ├── thinking.png
│           └── talking.png
```

> **제외 대상 (레거시/불필요 파일)**
> - `LiveCLI/Project`의 Python venv(수백MB), FastAPI 서버(`backend/main.py`), VOICEVOX 배치파일(`run.bat`, `stop.bat`), 구버전 OpenRouter 클라이언트
> - `Live-mate`의 임시 CSS 동그라미 얼굴 스타일/도형 마크업

---

## 3. 캐릭터 설정 스키마 (`characters.json`)

```json
{
  "activeCharacter": "mao",
  "characters": {
    "mao": {
      "name": "Mao (Live2D)",
      "type": "live2d",
      "modelPath": "characters/mao/Mao.model3.json",
      "reactions": {
        "idle": { "motionGroup": "Idle", "motionIndex": [0, 1], "expression": null, "subtitles": ["대기 중...", "쉬는 중이에요!"] },
        "thinking": { "motionGroup": "Idle", "motionIndex": 0, "expression": "exp_01", "subtitles": ["생각 중...", "어디 보자..."] },
        "talking": { "motionGroup": "TapBody", "motionIndex": 2, "expression": "exp_06", "subtitles": ["작업 완료!", "결과를 확인해줘!"] }
      }
    },
    "pixel_cat": {
      "name": "고양이 (2D Image)",
      "type": "image",
      "states": {
        "idle": { "src": "characters/pixel_cat/idle.png", "subtitles": ["(골골송 부르는 중)"] },
        "thinking": { "src": "characters/pixel_cat/thinking.png", "subtitles": ["(화면 쳐다보는 중)"] },
        "talking": { "src": "characters/pixel_cat/talking.png", "subtitles": ["야옹! (완료)"] }
      }
    }
  }
}
```

---

## 4. 검증 계획 (Verification Plan)

1. **빌드 검증**: `cargo check` 및 Tauri 개발 빌드 정상 구동 확인
2. **Live2D 캐릭터 구동 및 모션 확인**: Mao 모델 렌더링, Claude 작업 상태(`idle`/`thinking`/`talking`)에 따른 모션/표정/자막 전환 확인
3. **2D 이미지 캐릭터 전환 확인**: 단축키(Tab 또는 C)를 눌렀을 때 2D 이미지 캐릭터로 즉시 전환되고 정상 동작하는지 확인
4. **마우스 상호작용 검증**: 캐릭터 형태(Live2D/2D)와 무관하게 캐릭터 몸체 영역 드래그 이동 및 투명 빈 영역 클릭 관통 동작 확인
