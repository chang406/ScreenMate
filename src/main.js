const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const { getCurrentWindow } = window.__TAURI__.window;

const appWindow = getCurrentWindow();

const canvas = document.getElementById("live2d-canvas");
const imgEl = document.getElementById("image-character");
const speechBubble = document.getElementById("speech-bubble");
const bubbleText = document.getElementById("bubble-text");
const charNameEl = document.getElementById("char-name");
const switchBtn = document.getElementById("switch-btn");
const contextMenu = document.getElementById("context-menu");
const toggleTopBtn = document.getElementById("toggle-top-btn");
const topStatusText = document.getElementById("top-status-text");
const menuSwitchCharBtn = document.getElementById("menu-switch-char-btn");
const menuSettingsBtn = document.getElementById("menu-settings-btn");
const menuExitBtn = document.getElementById("menu-exit-btn");
const scaleBtns = document.querySelectorAll(".scale-btn");
const characterContainer = document.getElementById("character-container");

let config = null;
let currentKey = "mao";
let currentState = "idle";
let pixiApp = null;
let currentLive2DModel = null;
let lastIgnoringCursor = null;
let bubbleTimer = null;

let stateLockUntil = 0;
let pendingState = null;
let pendingDetail = null;
let pendingTimeout = null;

let busyRefreshInterval = null;
let lastBusySubtitleTime = 0;
let currentBusyDetail = null;
let talkingAutoIdleTimer = null;

// Busy 모션 주기 (기본 10초 쿨다운)
let busyMotionIntervalMs = parseInt(localStorage.getItem("screenmate_busy_motion_interval") || "10", 10) * 1000;
let lastBusyMotionTime = 0;

let currentScale = parseFloat(localStorage.getItem("screenmate_scale") || "1.0");
let isAlwaysOnTop = localStorage.getItem("screenmate_always_on_top") !== "false";

// 말풍선 설정 계산 및 스타일 적용
function getActiveBubbleSettings() {
  const globalSettings = config?.globalBubbleSettings || {
    autoTrack: true,
    offsetX: 0,
    offsetY: 0,
    duration: 3.5,
    fontSize: 13,
    bgColor: "#0f0f14",
    bgOpacity: 0.92,
    borderColor: "#404050",
    borderRadius: 14,
    hasTail: true
  };

  const charDef = config?.characters?.[currentKey];
  const charSettings = charDef?.bubbleSettings;

  if (charSettings && charSettings.useCustom !== false) {
    return {
      autoTrack: charSettings.autoTrack ?? globalSettings.autoTrack,
      offsetX: charSettings.offsetX ?? globalSettings.offsetX ?? 0,
      offsetY: charSettings.offsetY ?? globalSettings.offsetY ?? 0,
      duration: charSettings.duration ?? globalSettings.duration,
      fontSize: charSettings.fontSize ?? globalSettings.fontSize,
      bgColor: charSettings.bgColor ?? globalSettings.bgColor,
      bgOpacity: charSettings.bgOpacity ?? globalSettings.bgOpacity,
      borderColor: charSettings.borderColor ?? globalSettings.borderColor,
      borderRadius: charSettings.borderRadius ?? globalSettings.borderRadius,
      hasTail: charSettings.hasTail ?? globalSettings.hasTail,
    };
  }

  return globalSettings;
}

function hexToRgba(hex, alpha = 1) {
  if (!hex) return `rgba(15, 15, 20, ${alpha})`;
  let c = hex.replace("#", "");
  if (c.length === 3) c = c.split("").map((x) => x + x).join("");
  if (c.length !== 6) return hex;
  const num = parseInt(c, 16);
  const r = (num >> 16) & 255;
  const g = (num >> 8) & 255;
  const b = num & 255;
  return `rgba(${r}, ${g}, ${b}, ${alpha})`;
}

function applyBubbleStyles(overrideSettings = null) {
  if (!speechBubble) return;
  const s = overrideSettings || getActiveBubbleSettings();

  const bgRgba = hexToRgba(s.bgColor, s.bgOpacity ?? 0.92);
  const borderColor = s.borderColor || "rgba(255, 255, 255, 0.25)";

  speechBubble.style.setProperty("--bubble-bg", bgRgba);
  speechBubble.style.setProperty("--bubble-offset-x", `${s.offsetX || 0}px`);
  speechBubble.style.setProperty("--bubble-font-size", `${s.fontSize || 13}px`);
  speechBubble.style.setProperty("--bubble-radius", `${s.borderRadius || 14}px`);
  speechBubble.style.setProperty("--bubble-border", `1.5px solid ${borderColor}`);
  speechBubble.style.setProperty("--bubble-tail-color", bgRgba);
  speechBubble.style.setProperty("--bubble-tail-display", s.hasTail !== false ? "block" : "none");

  updateBubblePosition(s);
}

function updateBubblePosition(overrideSettings = null) {
  if (!speechBubble) return;
  const s = overrideSettings || getActiveBubbleSettings();
  const offsetY = s.offsetY || 0;

  // 자동 밀착 추적이 비활성화된 경우 고정 20px (+ 오프셋)
  if (!s.autoTrack) {
    const fixedTop = Math.max(2, 20 - offsetY);
    speechBubble.style.setProperty("--bubble-top", `${fixedTop}px`);
    speechBubble.style.setProperty("--bubble-bottom", "auto");
    return;
  }

  const charDef = config?.characters?.[currentKey];
  const winHeight = window.innerHeight || 920;
  const gap = 12; // 말풍선 꼬리와 머리 사이의 기본 간격

  let characterHeadY = 120;

  if (charDef?.type === "live2d" && currentLive2DModel) {
    // 실시간 모션/호흡에 의해 요동치지 않는 안정된 모델 기준선(model.y) 사용
    characterHeadY = currentLive2DModel.y !== undefined ? currentLive2DModel.y : 120;
  } else if (imgEl && !imgEl.classList.contains("hidden")) {
    const rect = imgEl.getBoundingClientRect();
    characterHeadY = rect.top;
  }

  // 뷰포트 바닥(Bottom) 기준 말풍선 바닥 위치 고정 (Bottom-Pinned)
  // 문장 길이에 따라 높이가 변해도 바닥과 꼬리표는 머리 위에 완벽 고정되고 위쪽으로만 확장됨
  const desiredBottom = (winHeight - characterHeadY) + gap + offsetY;

  speechBubble.style.setProperty("--bubble-top", "auto");
  speechBubble.style.setProperty("--bubble-bottom", `${Math.max(10, Math.round(desiredBottom))}px`);
}

function showSubtitle(text, durationOverride = null, overrideSettings = null) {
  if (!text) return;
  const s = overrideSettings || getActiveBubbleSettings();

  bubbleText.textContent = text;
  speechBubble.classList.remove("bubble-hidden");
  
  requestAnimationFrame(() => {
    updateBubblePosition(s);
  });
  
  if (bubbleTimer) {
    clearTimeout(bubbleTimer);
    bubbleTimer = null;
  }

  // durationOverride가 0이면(수면 모드 등) 자동으로 닫지 않고 계속 상시 유지
  if (durationOverride === 0 || durationOverride === Infinity) {
    return;
  }

  const duration = durationOverride ?? ((s.duration || 3.5) * 1000);
  bubbleTimer = setTimeout(() => {
    speechBubble.classList.add("bubble-hidden");
    if (overrideSettings) {
      applyBubbleStyles();
    }
  }, duration);
}

// 스마트 폴백 헬퍼: 캐릭터에 특정 상태가 없을 때 가장 어울리는 기본 상태 반환
function resolveFallbackState(charDef, state) {
  const isLive2d = charDef.type === "live2d";
  const dict = isLive2d ? charDef.reactions : charDef.states;
  if (!dict) return null;

  if (dict[state]) return { stateName: state, def: dict[state] };

  // Fallback 체인
  const fallbackMap = {
    "busy": ["thinking", "talking", "idle"],
    "error": ["thinking", "talking", "idle"],
    "click_react": ["talking", "idle", "thinking"],
    "sleeping": ["idle", "thinking"],
    "thinking": ["idle"],
    "talking": ["idle"]
  };

  const fallbacks = fallbackMap[state] || ["idle"];
  for (const fallback of fallbacks) {
    if (dict[fallback]) {
      return { stateName: fallback, def: dict[fallback] };
    }
  }

  // 최후의 수단: 등록된 첫 번째 상태
  const firstKey = Object.keys(dict)[0];
  return firstKey ? { stateName: firstKey, def: dict[firstKey] } : null;
}

const CONTEXTUAL_BUSY_SUBTITLES = {
  edit: [
    "열심히 코딩 중이에요! 🪄",
    "파일 작성 및 수정 중...",
    "코드 꼼꼼히 작성 중! ✨",
    "파일 변경 사항 반영 중..."
  ],
  command: [
    "터미널 명령어 실행 중... 💻",
    "명령어 실행 중...",
    "빌드/실행 확인 중! ⚡"
  ],
  read: [
    "파일을 꼼꼼히 읽는 중... 🔍",
    "코드 구조 분석 중이에요!",
    "파일 탐색 중... 📂",
    "관련 문서 확인 중..."
  ],
  browse: [
    "웹페이지 확인 중... 🌐",
    "브라우저 탐색 중..."
  ],
  general: [
    "열심히 작업 중이에요! 🪄",
    "작업 진행 중...",
    "잠시만 기다려 주세요!"
  ]
};

function getBusySubtitle(charDef, detail) {
  if (detail && CONTEXTUAL_BUSY_SUBTITLES[detail]) {
    const list = CONTEXTUAL_BUSY_SUBTITLES[detail];
    return list[Math.floor(Math.random() * list.length)];
  }
  const resolved = resolveFallbackState(charDef, "busy");
  const subList = resolved?.def?.subtitles;
  if (subList && subList.length > 0) {
    return subList[Math.floor(Math.random() * subList.length)];
  }
  return "열심히 작업 중이에요! 🪄";
}

function stopBusyRefresh() {
  if (busyRefreshInterval) {
    clearInterval(busyRefreshInterval);
    busyRefreshInterval = null;
  }
}

function startBusyRefresh() {
  stopBusyRefresh();
  busyRefreshInterval = setInterval(() => {
    if (currentState === "busy") {
      const charDef = config?.characters[currentKey];
      if (charDef) {
        // 1. 말풍선 대사 주기적 갱신
        const sub = getBusySubtitle(charDef, currentBusyDetail);
        showSubtitle(sub, 3500);
        lastBusySubtitleTime = Date.now();

        // 2. Busy 모션은 10초 주기가 지났을 때만 실행
        const now = Date.now();
        if (now - lastBusyMotionTime >= busyMotionIntervalMs) {
          lastBusyMotionTime = now;
          if (charDef.type === "live2d" && currentLive2DModel) {
            const resolved = resolveFallbackState(charDef, "busy");
            if (resolved?.def?.motionGroup) {
              try {
                const idx = Array.isArray(resolved.def.motionIndex)
                  ? resolved.def.motionIndex[Math.floor(Math.random() * resolved.def.motionIndex.length)]
                  : resolved.def.motionIndex ?? 0;
                currentLive2DModel.motion(resolved.def.motionGroup, idx);
              } catch (_) {}
            }
          }
        }
      }
    } else {
      stopBusyRefresh();
    }
  }, 5000);
}

function apply2DImageState(charDef, state, detail = null) {
  const resolved = resolveFallbackState(charDef, state);
  if (!resolved) return;

  const stateDef = resolved.def;
  imgEl.src = stateDef.src;
  imgEl.className = "state-" + resolved.stateName;

  if (state === "busy") {
    const sub = getBusySubtitle(charDef, detail);
    showSubtitle(sub, 3500);
  } else if (stateDef.subtitles && stateDef.subtitles.length > 0) {
    const randomSub = stateDef.subtitles[Math.floor(Math.random() * stateDef.subtitles.length)];
    const duration = state === "talking" ? 4500 : (state === "sleeping" ? 0 : 3200);
    showSubtitle(randomSub, duration);
  }
}

function applyLive2DState(charDef, state, detail = null) {
  if (!currentLive2DModel) return;
  const resolved = resolveFallbackState(charDef, state);
  if (!resolved) return;

  const reaction = resolved.def;
  const now = Date.now();

  // 모션 재생 판단: busy 상태일 때는 10초 쿨다운을 검사하고, 다른 상태(talking, click_react 등)는 즉시 재생
  const shouldPlayMotion = (state !== "busy") || (now - lastBusyMotionTime >= busyMotionIntervalMs);

  if (reaction.motionGroup && shouldPlayMotion) {
    if (state === "busy") {
      lastBusyMotionTime = now;
    }
    const idx = Array.isArray(reaction.motionIndex)
      ? reaction.motionIndex[Math.floor(Math.random() * reaction.motionIndex.length)]
      : reaction.motionIndex ?? 0;
    try {
      currentLive2DModel.motion(reaction.motionGroup, idx);
    } catch (e) {
      console.warn("Motion fallback:", e);
      try { currentLive2DModel.motion("Idle", 0); } catch (_) {}
    }
  }

  if (reaction.expressions && Array.isArray(reaction.expressions) && reaction.expressions.length > 0) {
    const exp = reaction.expressions[Math.floor(Math.random() * reaction.expressions.length)];
    if (exp) {
      try { currentLive2DModel.expression(exp); } catch (e) {}
    }
  } else if (reaction.expression !== undefined && reaction.expression !== "") {
    try { currentLive2DModel.expression(reaction.expression); } catch (e) {}
  }

  if (state === "busy") {
    const sub = getBusySubtitle(charDef, detail);
    showSubtitle(sub, 3500);
  } else if (reaction.subtitles && reaction.subtitles.length > 0) {
    const randomSub = reaction.subtitles[Math.floor(Math.random() * reaction.subtitles.length)];
    const duration = state === "talking" ? 4500 : (state === "sleeping" ? 0 : 3200);
    showSubtitle(randomSub, duration);
  }
}

function renderState(state, force = false, detail = null) {
  if (currentState === state && !force) {
    return;
  }
  const prevState = currentState;
  currentState = state;
  const charDef = config?.characters[currentKey];
  if (!charDef) return;

  if (prevState === "sleeping" && state !== "sleeping") {
    // 수면 모드에서 벗어날 때 열려있던 상시 말풍선 닫기 처리
    if (bubbleTimer) {
      clearTimeout(bubbleTimer);
      bubbleTimer = null;
    }
    if (state === "idle") {
      speechBubble.classList.add("bubble-hidden");
    }
  }

  if (state === "busy") {
    if (prevState !== "busy") {
      lastBusyMotionTime = 0; // busy 최초 진입 시 즉시 1회 모션 보장
    }
    currentBusyDetail = detail || currentBusyDetail || "general";
    lastBusySubtitleTime = Date.now();
    startBusyRefresh();
  } else {
    stopBusyRefresh();
  }

  if (talkingAutoIdleTimer) {
    clearTimeout(talkingAutoIdleTimer);
    talkingAutoIdleTimer = null;
  }

  if (state === "talking") {
    // 2D 이미지 및 Live2D 공통: 말풍선 노출 완료(4.5초) 후 자동으로 idle 대기 상태로 복귀
    talkingAutoIdleTimer = setTimeout(() => {
      if (currentState === "talking") {
        renderState("idle");
      }
    }, 4500);
  }

  console.log("[ScreenMate] state:", state, "detail:", detail);
  if (charDef.type === "live2d") {
    applyLive2DState(charDef, state, detail);
  } else {
    apply2DImageState(charDef, state, detail);
  }
}

function queueState(targetState, detail = null) {
  const now = Date.now();

  // 이미 busy 상태에서 새로운 도구 이벤트(detail)가 들어왔을 때
  if (targetState === "busy" && currentState === "busy") {
    if (detail) {
      currentBusyDetail = detail;
    }
    // 이전 말풍선이 뜬 지 3.5초 이상 지났다면 새로운 맞춤 대사로 부드럽게 갱신 (모션은 10초 쿨다운 유지)
    if (now - lastBusySubtitleTime >= 3500) {
      lastBusySubtitleTime = now;
      const charDef = config?.characters[currentKey];
      if (charDef) {
        const sub = getBusySubtitle(charDef, currentBusyDetail);
        showSubtitle(sub, 3500);
        if (now - lastBusyMotionTime >= busyMotionIntervalMs) {
          lastBusyMotionTime = now;
          if (charDef.type === "live2d" && currentLive2DModel) {
            const resolved = resolveFallbackState(charDef, "busy");
            if (resolved?.def?.motionGroup) {
              try {
                const idx = Array.isArray(resolved.def.motionIndex)
                  ? resolved.def.motionIndex[Math.floor(Math.random() * resolved.def.motionIndex.length)]
                  : resolved.def.motionIndex ?? 0;
                currentLive2DModel.motion(resolved.def.motionGroup, idx);
              } catch (_) {}
            }
          }
        }
      }
    }
    return;
  }

  // 이미 해당 상태이고 대기 중인 상태 전환이 없으면 무시
  if (targetState === currentState && !pendingState) {
    return;
  }

  // thinking 상태 시작: 최소 1.5초간 생각 모션을 보장하도록 잠금
  if (targetState === "thinking") {
    stateLockUntil = now + 1500;
    if (pendingTimeout) {
      clearTimeout(pendingTimeout);
      pendingTimeout = null;
    }
    pendingState = null;
    pendingDetail = null;
    renderState(targetState, false, detail);
    return;
  }

  // 아직 thinking 모션 보장 시간 내라면, 보장 종료 후 순차 전환되도록 대기열에 저장
  if (now < stateLockUntil) {
    pendingState = targetState;
    pendingDetail = detail;
    if (!pendingTimeout) {
      const delay = stateLockUntil - now;
      pendingTimeout = setTimeout(() => {
        pendingTimeout = null;
        if (pendingState) {
          const nextState = pendingState;
          const nextDetail = pendingDetail;
          pendingState = null;
          pendingDetail = null;
          renderState(nextState, false, nextDetail);
        }
      }, delay);
    }
    return;
  }

  // 보장 시간이 지났다면 즉시 전환
  if (pendingTimeout) {
    clearTimeout(pendingTimeout);
    pendingTimeout = null;
  }
  pendingState = null;
  pendingDetail = null;
  renderState(targetState, false, detail);
}

function fitModelToCanvas() {
  if (!currentLive2DModel || !canvas) return;
  const model = currentLive2DModel;
  model.scale.set(1);
  const naturalWidth = model.width || 500;
  const naturalHeight = model.height || 600;

  // 100% 기준 캐릭터 기본 비율 (640x920 창에서 약 46% 차지하여 200% 확대 시에도 상단 말풍선 공간 완벽 확보)
  const baseScale = Math.min(
    (canvas.width * 0.46) / naturalWidth,
    (canvas.height * 0.46) / naturalHeight
  );

  const finalScale = baseScale * currentScale;
  model.scale.set(finalScale);
  model.x = (canvas.width - naturalWidth * finalScale) / 2;
  model.y = canvas.height - naturalHeight * finalScale - 16;
}

async function setScale(scale, notify = true) {
  currentScale = Math.min(Math.max(scale, 0.6), 2.0);
  localStorage.setItem("screenmate_scale", currentScale.toString());

  // 윈도우 창 크기는 640x920으로 넉넉하게 고정하고, 캐릭터 모델만 내부에서 배율 조절
  if (currentLive2DModel) {
    fitModelToCanvas();
  }

  if (imgEl) {
    imgEl.style.transform = `scale(${currentScale})`;
    imgEl.style.transformOrigin = "bottom center";
  }

  updateBubblePosition();

  scaleBtns.forEach((btn) => {
    const val = parseFloat(btn.dataset.scale);
    if (Math.abs(val - currentScale) < 0.05) {
      btn.classList.add("active");
    } else {
      btn.classList.remove("active");
    }
  });

  if (notify) {
    showSubtitle(`크기: ${Math.round(currentScale * 100)}%`, 1500);
  }
}

async function toggleAlwaysOnTop() {
  isAlwaysOnTop = !isAlwaysOnTop;
  localStorage.setItem("screenmate_always_on_top", isAlwaysOnTop.toString());

  try {
    await invoke("set_always_on_top", { alwaysOnTop: isAlwaysOnTop });
    updateAlwaysOnTopUI();
    showSubtitle(isAlwaysOnTop ? "📌 항상 위에 고정됨" : "⬇️ 항상 위 고정 해제됨", 2000);
  } catch (e) {
    console.error("set_always_on_top failed:", e);
  }
}

function updateAlwaysOnTopUI() {
  topStatusText.textContent = isAlwaysOnTop ? "항상 위에 고정: ON" : "항상 위에 고정: OFF";
  toggleTopBtn.style.opacity = isAlwaysOnTop ? "1" : "0.7";
}

/** Live2D 런타임(Cubism Core + Pixi + 플러그인) 로드 결과. lib/live2d-loader.js 참고 */
async function getLive2DRuntime() {
  if (window.__live2dReady) {
    try {
      return await window.__live2dReady;
    } catch (e) {
      return { ok: false, error: String(e?.message || e) };
    }
  }
  const ok = typeof PIXI !== "undefined" && !!PIXI.live2d;
  return { ok, error: ok ? null : "Live2D 런타임 없음" };
}

/** Live2D를 띄울 수 없을 때 대신 보여줄 2D 캐릭터 키 (냥이 우선) */
function findImageFallbackKey() {
  const chars = config?.characters || {};
  if (chars.pixel_cat && chars.pixel_cat.type !== "live2d") return "pixel_cat";
  return Object.keys(chars).find((k) => chars[k]?.type !== "live2d") || null;
}

async function loadLive2D(charDef) {
  const runtime = await getLive2DRuntime();
  if (!runtime.ok) {
    console.warn("[ScreenMate] Live2D runtime unavailable:", runtime.error);
    return { ok: false, reason: runtime.error || "Live2D 런타임 없음" };
  }

  canvas.classList.remove("hidden");
  imgEl.classList.add("hidden");

  if (!pixiApp) {
    try {
      pixiApp = new PIXI.Application({
        view: canvas,
        width: window.innerWidth,
        height: window.innerHeight,
        backgroundAlpha: 0,
        autoDensity: true,
        resolution: window.devicePixelRatio || 1,
      });
    } catch (err) {
      console.error("PIXI init failed:", err);
      return { ok: false, reason: "그래픽 초기화 실패: " + (err?.message || err) };
    }
  }

  if (currentLive2DModel) {
    pixiApp.stage.removeChild(currentLive2DModel);
    currentLive2DModel.destroy();
    currentLive2DModel = null;
  }

  try {
    const model = await PIXI.live2d.Live2DModel.from(charDef.modelPath);
    pixiApp.stage.addChild(model);
    currentLive2DModel = model;
    fitModelToCanvas();
    renderState(currentState, true);
    return { ok: true };
  } catch (err) {
    console.error("Live2D load failed:", err);
    let hint = "";
    try {
      // 모델(moc3)이 Cubism Core보다 최신이면 'Unknown error'만 나오므로 지원 버전을 함께 안내
      const latest = window.Live2DCubismCore?.Version?.csmGetLatestMocVersion?.();
      if (latest !== undefined) hint = ` · 현재 Cubism Core가 지원하는 moc3 버전 ≤ ${latest}, 최신 Core가 필요할 수 있어요`;
    } catch (_) {}
    return { ok: false, reason: "모델 로드 실패: " + (err?.message || err) + hint };
  }
}

function load2DImage(charDef) {
  canvas.classList.add("hidden");
  imgEl.classList.remove("hidden");

  if (pixiApp && currentLive2DModel) {
    pixiApp.stage.removeChild(currentLive2DModel);
    currentLive2DModel.destroy();
    currentLive2DModel = null;
  }

  renderState(currentState, true);
  return true;
}

async function switchCharacter(key) {
  if (!config || !config.characters[key]) return;
  currentKey = key;
  const charDef = config.characters[key];
  charNameEl.textContent = charDef.name || key;

  if (charDef.type === "live2d") {
    const result = await loadLive2D(charDef);
    if (!result.ok) {
      // 캐릭터가 통째로 사라지지 않도록 2D 캐릭터로 임시 대체 (설정은 바꾸지 않음 → 다음 실행 때 다시 시도)
      const fallbackKey = findImageFallbackKey();
      if (fallbackKey && fallbackKey !== key) {
        currentKey = fallbackKey;
        const fb = config.characters[fallbackKey];
        charNameEl.textContent = fb.name || fallbackKey;
        load2DImage(fb);
        applyBubbleStyles();
        showSubtitle(`⚠️ ${charDef.name || key}을(를) 띄울 수 없어 ${fb.name || fallbackKey}(으)로 대신 나왔어요 (${result.reason})`, 6000);
      } else {
        applyBubbleStyles();
        showSubtitle("⚠️ Live2D 로드 실패: " + result.reason, 6000);
      }
      return;
    }
  } else {
    load2DImage(charDef);
  }
  applyBubbleStyles();
  showSubtitle(charDef.name + " 준비 완료!", 2500);
}

async function switchRandomCharacter() {
  if (!config || !config.characters) return;
  const keys = Object.keys(config.characters);
  if (keys.length === 0) return;

  let targetKey = keys[0];
  if (keys.length > 1) {
    const otherKeys = keys.filter((k) => k !== currentKey);
    targetKey = otherKeys[Math.floor(Math.random() * otherKeys.length)];
  }

  try {
    await invoke("set_active_character", { id: targetKey });
  } catch (e) {
    console.error("set_active_character failed:", e);
    await switchCharacter(targetKey);
  }
}

function isOverCharacter(x, y) {
  if (!contextMenu.classList.contains("hidden")) {
    const menuRect = contextMenu.getBoundingClientRect();
    if (
      x >= menuRect.left &&
      x <= menuRect.right &&
      y >= menuRect.top &&
      y <= menuRect.bottom
    ) {
      return true;
    }
  }

  const charDef = config?.characters[currentKey];
  if (!charDef) return false;

  if (charDef.type === "live2d" && currentLive2DModel) {
    const naturalWidth = currentLive2DModel.width || 500;
    const naturalHeight = currentLive2DModel.height || 600;
    const baseScale = Math.min(
      (canvas.width * 0.46) / naturalWidth,
      (canvas.height * 0.46) / naturalHeight
    );
    const finalScale = baseScale * currentScale;
    const charW = naturalWidth * finalScale;
    const charH = naturalHeight * finalScale;

    const charLeft = (canvas.clientWidth - charW) / 2 + charW * 0.08;
    const charRight = (canvas.clientWidth + charW) / 2 - charW * 0.08;
    const charTop = canvas.clientHeight - charH - 16 + charH * 0.05;
    const charBottom = canvas.clientHeight - 8;
    return x >= charLeft && x <= charRight && y >= charTop && y <= charBottom;
  } else {
    const rect = imgEl.getBoundingClientRect();
    return (
      x >= rect.left &&
      x <= rect.right &&
      y >= rect.top &&
      y <= rect.bottom
    );
  }
}

async function setIgnoringCursor(ignore) {
  if (ignore === lastIgnoringCursor) return;
  lastIgnoringCursor = ignore;
  try {
    await invoke("set_click_through", { ignore });
  } catch (err) {
    console.error("set_click_through failed:", err);
  }
}

listen("cursor-position", (event) => {
  const { x, y, inside } = event.payload;
  if (!inside) {
    setIgnoringCursor(true);
    return;
  }
  const dpr = window.devicePixelRatio || 1;
  const onChar = isOverCharacter(x / dpr, y / dpr);
  setIgnoringCursor(!onChar);
});

// 마우스 클릭 / 드래그 판정 변수
let isMouseDown = false;
let isDragging = false;
let mouseDownX = 0;
let mouseDownY = 0;
let mouseDownTime = 0;
let clickReactTimer = null;

function triggerClickReact() {
  renderState("click_react", true);

  if (clickReactTimer) clearTimeout(clickReactTimer);
  clickReactTimer = setTimeout(() => {
    if (currentState === "click_react") {
      renderState("idle");
    }
  }, 3500);
}

document.addEventListener("mousedown", (event) => {
  if (event.button === 0) {
    if (!contextMenu.contains(event.target)) {
      contextMenu.classList.add("hidden");
    }

    if (event.target === switchBtn || contextMenu.contains(event.target)) {
      return;
    }

    isMouseDown = true;
    isDragging = false;
    mouseDownX = event.screenX;
    mouseDownY = event.screenY;
    mouseDownTime = Date.now();
  }
});

document.addEventListener("mousemove", (event) => {
  if (isMouseDown && !isDragging) {
    const dx = Math.abs(event.screenX - mouseDownX);
    const dy = Math.abs(event.screenY - mouseDownY);

    // 6px 이상 움직였을 때만 창 드래그 시작 (Windows DefWindowProc로 전환)
    if (dx > 6 || dy > 6) {
      isDragging = true;
      isMouseDown = false;
      appWindow.startDragging().catch((err) => {
        console.error("startDragging failed:", err);
      });
    }
  }
});

// 클릭(탭) 인터랙션: 드래그하지 않고 제자리에서 뗐을 때 터치 반응 실행
document.addEventListener("mouseup", (event) => {
  if (event.button === 0 && isMouseDown) {
    isMouseDown = false;
    const dt = Date.now() - mouseDownTime;

    if (!isDragging && dt < 500) {
      if (event.target !== switchBtn && !contextMenu.contains(event.target)) {
        triggerClickReact();
      }
    }
    isDragging = false;
  }
});

document.addEventListener("wheel", (e) => {
  e.preventDefault();
  const delta = e.deltaY < 0 ? 0.05 : -0.05;
  setScale(currentScale + delta);
}, { passive: false });

document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  
  // 1. 메뉴를 표시하고 위치 초기화
  contextMenu.style.left = "0px";
  contextMenu.style.top = "0px";
  contextMenu.classList.remove("hidden");
  
  // 2. 정확한 렌더링 Bounding Rect 측정
  const rect = contextMenu.getBoundingClientRect();
  const menuWidth = rect.width > 0 ? rect.width : 220;
  const menuHeight = rect.height > 0 ? rect.height : 265;
  const winWidth = window.innerWidth;
  const winHeight = window.innerHeight;

  let posX = e.clientX;
  let posY = e.clientY;

  // 우측 화면 밖으로 벗어남 방지 (안전 여백 12px)
  if (posX + menuWidth > winWidth - 12) {
    posX = winWidth - menuWidth - 12;
  }
  if (posX < 12) {
    posX = 12;
  }

  // 하단 화면 밖으로 벗어남 방지 (안전 여백 12px)
  if (posY + menuHeight > winHeight - 12) {
    posY = winHeight - menuHeight - 12;
  }
  if (posY < 12) {
    posY = 12;
  }

  contextMenu.style.left = `${Math.round(posX)}px`;
  contextMenu.style.top = `${Math.round(posY)}px`;
});

scaleBtns.forEach((btn) => {
  btn.addEventListener("click", (e) => {
    e.stopPropagation();
    setScale(parseFloat(btn.dataset.scale));
  });
});

toggleTopBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  toggleAlwaysOnTop();
});

menuSwitchCharBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  contextMenu.classList.add("hidden");
  switchRandomCharacter();
});

menuSettingsBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  contextMenu.classList.add("hidden");
  invoke("open_settings_window");
});

menuExitBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  invoke("exit_app");
});

switchBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  switchRandomCharacter();
});

window.addEventListener("resize", () => {
  if (pixiApp) {
    pixiApp.renderer.resize(window.innerWidth, window.innerHeight);
    fitModelToCanvas();
    updateBubblePosition();
  }
});

listen("state-change", (event) => {
  console.log("[state-change event]", event.payload.state, event.payload.detail);
  queueState(event.payload.state, event.payload.detail);
});

async function reloadConfig() {
  try {
    config = await invoke("get_character_configs");
  } catch (e) {
    console.error("reloadConfig failed:", e);
  }
}

listen("characters-updated", async () => {
  console.log("[characters-updated event]");
  await reloadConfig();
  applyBubbleStyles();
});

listen("bubble-settings-updated", async () => {
  console.log("[bubble-settings-updated event]");
  await reloadConfig();
  applyBubbleStyles();
});

listen("test-bubble", (event) => {
  console.log("[test-bubble event]", event.payload);
  const data = event.payload || {};
  const text = (typeof data === "string" ? data : data.text) || "💬 말풍선 미리보기 테스트입니다! ✨";
  const customSettings = (typeof data === "object" && data.settings) ? data.settings : null;

  applyBubbleStyles(customSettings);
  const durationMs = customSettings?.duration ? Math.round(customSettings.duration * 1000) : 3500;
  showSubtitle(text, durationMs, customSettings);
});

listen("switch-character", async (event) => {
  console.log("[switch-character event]", event.payload);
  const targetKey = event.payload;
  await reloadConfig();
  if (targetKey && config?.characters[targetKey]) {
    await switchCharacter(targetKey);
  }
});

async function init() {
  try {
    config = await invoke("get_character_configs");
    const activeKey = config?.activeCharacter || "mao";
    await switchCharacter(activeKey);
    setScale(currentScale, false);
    updateAlwaysOnTopUI();
    applyBubbleStyles();
  } catch (e) {
    console.error("Init failed:", e);
  }
}

init();

