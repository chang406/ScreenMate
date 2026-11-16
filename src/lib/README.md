# 📦 src/lib — 오프라인 런타임 라이브러리

ScreenMate 오버레이(`src/index.html`)는 `live2d-loader.js`를 통해 아래 순서로 Live2D 런타임을 불러옵니다.
**로컬 파일이 있으면 로컬을, 없으면 CDN을** 사용하므로 인터넷이 없어도 앱이 멈추지 않습니다.

| 파일 | 라이선스 | 레포 포함 | 비고 |
| :--- | :--- | :---: | :--- |
| `vendor/pixi.min.js` (pixi.js 6.5.9) | MIT (`vendor/LICENSE-pixi.js.txt`) | ✅ | npm `pixi.js@6.5.9` `dist/browser/pixi.min.js` 원본 |
| `vendor/pixi-live2d-display.cubism4.min.js` (0.4.0) | MIT (`vendor/LICENSE-pixi-live2d-display.txt`) | ✅ | npm `pixi-live2d-display@0.4.0` `dist/cubism4.min.js` 원본 |
| `live2dcubismcore.min.js` (Live2D Cubism Core) | Live2D Proprietary Software License | ❌ (`.gitignore`) | 직접 배치 시 오프라인 동작, 없으면 공식 CDN 사용 |

## 완전 오프라인(폐쇄망)에서 Live2D 캐릭터 쓰기

1. Live2D 공식 사이트에서 **Cubism SDK for Web**을 내려받습니다(라이선스 동의 필요).
2. SDK 안의 `Core/live2dcubismcore.min.js`를 이 폴더(`src/lib/live2dcubismcore.min.js`)에 복사합니다.
3. 앱을 재시작합니다. 개발자 콘솔에 `[live2d-loader] {"ok":true, ... "core":"lib/live2dcubismcore.min.js"}`가 찍히면 성공입니다.

> ⚠️ **Core 버전 주의**: 기본 캐릭터 `mao`는 Cubism 5 모델(moc3 v5)이므로 **최신 Core**가 필요합니다.
> 오래된 Core를 넣으면 Mao는 `모델 로드 실패: Unknown error`로 실패하고 2D 캐릭터로 대체됩니다. (`wanko`는 구버전 Core로도 동작)

## Live2D 런타임을 못 불러오면?

캐릭터가 사라지는 대신 **2D 캐릭터(냥이)로 임시 대체**되고 말풍선으로 이유를 알려 줍니다.
설정(`activeCharacter`)은 바뀌지 않으므로, 네트워크/Core가 갖춰지면 다음 실행부터 원래 캐릭터로 돌아옵니다.

## 라이브러리 무결성 (SHA-256)

```
1b5f0b098d45e707f637cae17085e2d0b08231b05e0bdef38efae827c031883a  vendor/pixi.min.js
af1267e6d52759b245766c578d905bfa025b532d5c3cc727c370957c4409e21b  vendor/pixi-live2d-display.cubism4.min.js
```
