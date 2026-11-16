# 🎨 Custom Characters Guide (나만의 캐릭터 가이드)

이 디렉토리는 사용자가 개인적으로 좋아하는 캐릭터(2D 이미지, GIF, SVG, Live2D 모델)를 추가하여 ScreenMate의 오버레이 파트너로 사용할 수 있는 공간입니다.

---

## 🛡️ 저작권 및 Git 자동 보호 원리

- **GitHub 커밋 자동 방지**:
  - `custom_characters/*` 하위의 모든 사용자 폴더(예: `my_char/`, `my_live2d/` 등)는 프로젝트 루트의 `.gitignore`에 등록되어 있어 **GitHub 등 원격 저장소로 절대 커밋되거나 푸시되지 않습니다.**
  - 단, 다른 사용자들을 위한 스타터 템플릿인 **`_template/` 폴더와 본 안내 문서만 Git 예외(`!`)로 관리**됩니다.
- **안전한 로컬 전용 사용**:
  - 외부 게임/애니메이션 등 좋아하는 캐릭터를 개인 로컬 PC에서 저작권 걱정 없이 안심하고 즐기실 수 있습니다.

---

## 🚀 3분 만에 나만의 캐릭터 만들기 (Step-by-Step)

### Step 1. 템플릿 폴더 복사
`_template/` 폴더를 복사하여 같은 디렉토리에 원하는 이름(예: `my_bot`)으로 새 폴더를 만듭니다.
```text
src/characters/custom_characters/
├── _template/          <-- 원본 템플릿
└── my_bot/             <-- 새로 복사한 내 캐릭터 폴더!
    ├── idle.svg
    ├── thinking.svg
    ├── talking.svg
    └── character_snippet.json
```

### Step 2. 이미지 파일 교체
`my_bot/` 폴더 안의 가이드 이미지를 준비하신 캐릭터 이미지(PNG, GIF, SVG 등)로 덮어씁니다.
- `idle.png` (대기 상태)
- `thinking.png` (생각 중 상태)
- `talking.png` (완료 상태)
> 💡 **스마트 폴백**: 7가지 상태를 다 채울 필요 없이 **기본 3장(`idle`, `thinking`, `talking`)만 넣어도 완벽하게 작동**합니다! (단 1장만 넣어도 가능)

### Step 3. `character.json` 설정 (자동 인식)
더 이상 최상단 `characters.json`을 직접 수정할 필요가 없습니다!
`my_bot/character.json` 파일을 열고 캐릭터 이름과 원하는 대사를 작성하면 끝납니다:
```json
{
  "name": "나만의 도트 봇",
  "type": "image",
  "states": {
    "idle": {
      "src": "characters/custom_characters/my_bot/idle.png",
      "subtitles": ["대기 중이에요~", "무엇이든 물어보세요!"]
    },
    "thinking": {
      "src": "characters/custom_characters/my_bot/thinking.png",
      "subtitles": ["생각 중이에요...", "분석하고 있어요!"]
    },
    "talking": {
      "src": "characters/custom_characters/my_bot/talking.png",
      "subtitles": ["작업 완료! 확인해 보세요! ✨", "다 만들었어요!"]
    }
  }
}
```
> 🚀 **폴더 기반 자동 인식 (Self-Contained Mod Architecture)**:
> ScreenMate 백엔드가 실행 시 `custom_characters/` 하위 폴더들을 자동 스캔하므로, 폴더 안에 `character.json`만 있으면 즉시 캐릭터 매니저와 선택 메뉴에 나타납니다!

---

## 🛠️ 작업 중 도구별 맞춤 대사 (`contextualSubtitles`, 선택)

AI 에이전트가 작업 중(`busy`)일 때, **지금 무슨 도구를 쓰는지**에 맞춰 캐릭터 말투로 대사를 바꿀 수 있습니다.
`busy` 상태 안에 `contextualSubtitles`를 넣으면 됩니다(이미지가 없으면 `src`는 `thinking` 이미지를 써도 됩니다).

```json
"busy": {
  "src": "characters/custom_characters/my_bot/thinking.png",
  "subtitles": ["열심히 작업 중이에요! 🪄"],
  "contextualSubtitles": {
    "read":    ["파일 읽는 중이에요… 🔍"],
    "edit":    ["코드 고치는 중이에요! ✍️"],
    "command": ["명령어 실행 중이에요! 💻"],
    "browse":  ["웹 살펴보는 중이에요 🌐"]
  }
}
```

| 키 | 언제 | 예시 도구 |
| :--- | :--- | :--- |
| `read` | 파일 읽기·검색 | Read, Grep, 파일 탐색 |
| `edit` | 파일 작성·수정 | Edit, Write, apply_patch |
| `command` | 터미널 명령 실행 | Bash, shell |
| `browse` | 웹 탐색 | WebFetch, 브라우저 |
| `general` | 위에 해당하지 않는 작업 (선택) | 기타 도구 |

**대사 선택 순서** (없는 항목은 건너뜀):
1. 캐릭터의 `contextualSubtitles[도구]`
2. 공용 기본 도구별 대사 (예: *"파일을 꼼꼼히 읽는 중… 🔍"*)
3. 캐릭터의 `busy.subtitles` (도구 구분이 없는 일반 작업)
4. 공용 기본 문구

> 💡 **넣지 않아도 됩니다.** 없으면 공용 기본 도구별 대사가 자동으로 쓰입니다. 넣으면 캐릭터 말투 + 지금 하는 일이 함께 표현됩니다.
> 기본 캐릭터 `mao`, `wanko`, `pixel_cat`의 `character.json`에 예시가 들어 있습니다.

---

## 📋 7대 상태(State) 상세 가이드 표

캐릭터의 디테일을 높이고 싶다면 아래 7가지 상태에 맞춰 이미지를 추가할 수 있습니다:

| 상태 (State) | 발동 타이밍 (Trigger) | 권장 표정 및 모션 묘사 | 권장 대사 뉘앙스 | 폴백 순위 (미등록 시) |
| :--- | :--- | :--- | :--- | :--- |
| **`idle`** ⭐ | 대기 / 평상시 | 편안한 호흡, 잔잔한 미소, 정면 응시 | *"대기 중이에요~"*, *"쉬는 중..."* | **(최종 기본값)** |
| **`thinking`** ⭐ | 질문 분석 / 프롬프트 추론 중 | 턱을 괴고 고민, 갸웃거림, 돋보기, 집중 | *"어디 보자... 생각 중!"*, *"분석 중..."* | ➡️ `idle` |
| **`talking`** ⭐ | 작업 완료 / 성공 답변 출력 | 환호, 점프, 밝은 웃음, 쨘! 포즈 | *"작업 완료! 확인해 주세요! ✨"* | ➡️ `idle` |
| **`busy`** | 도구(Tool) / 터미널 명령어 실행 중 | 키보드 폭풍 타이핑, 마법봉 휘두르기, 작업 중 | *"열심히 코딩 중이에요! 🪄"*, *"실행 중!"* (도구별 대사는 `contextualSubtitles`) | ➡️ `thinking` ➡️ `idle` |
| **`error`** | 터미널 에러 / 실패 응답 발생 | 깜짝 놀람, 땀 흘림, 곤란한 표정 | *"어라? 에러가 발생했어요! 💦"* | ➡️ `thinking` ➡️ `idle` |
| **`click_react`** | 사용자가 마우스로 캐릭터 클릭 | 윙크, 손인사, 하트, 응원 제스처 | *"화이팅이에요! 힘내세요! 💕"* | ➡️ `talking` ➡️ `idle` |
| **`sleeping`** | 5분(300초) 이상 미활동 시 | 눈 감고 졸기, 침 흘림, 수면 모자 | *"Zzz... (새근새근)"*, *"쿨쿨... 💤"* | ➡️ `idle` |

> ⭐ 표시된 3가지(`idle`, `thinking`, `talking`)가 핵심 필수 상태입니다.

---

## 💡 이미지 제작 및 에셋 팁

1. **배경 투명화 (Alpha Channel)**:
   - 반드시 **배경이 투명한 PNG, WebP, SVG 또는 투명 GIF**를 사용하세요. (배경이 투명해야 마우스 클릭 관통이 정상 동작합니다.)
2. **권장 해상도 비율**:
   - 정사각형(1:1) 또는 3:4 비율을 권장합니다. (예: 256x256, 512x512)
3. **애니메이션 GIF 지원**:
   - 모션이 있는 움직이는 캐릭터를 원하시면 `idle.gif`, `thinking.gif`처럼 GIF 파일을 그대로 사용하실 수 있습니다!
