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
let pendingTimeout = null;

let currentScale = parseFloat(localStorage.getItem("screenmate_scale") || "1.0");
let isAlwaysOnTop = localStorage.getItem("screenmate_always_on_top") !== "false";

function showSubtitle(text, duration = 3500) {
  if (!text) return;
  bubbleText.textContent = text;
  speechBubble.classList.remove("bubble-hidden");
  
  if (bubbleTimer) clearTimeout(bubbleTimer);
  bubbleTimer = setTimeout(() => {
    speechBubble.classList.add("bubble-hidden");
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

function apply2DImageState(charDef, state) {
  const resolved = resolveFallbackState(charDef, state);
  if (!resolved) return;

  const stateDef = resolved.def;
  imgEl.src = stateDef.src;
  imgEl.className = "state-" + resolved.stateName;

  if (stateDef.subtitles && stateDef.subtitles.length > 0) {
    const randomSub = stateDef.subtitles[Math.floor(Math.random() * stateDef.subtitles.length)];
    showSubtitle(randomSub, state === "talking" ? 4500 : 3200);
  }
}

function applyLive2DState(charDef, state) {
  if (!currentLive2DModel) return;
  const resolved = resolveFallbackState(charDef, state);
  if (!resolved) return;

  const reaction = resolved.def;

  if (reaction.motionGroup) {
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

  if (reaction.expressions && Array.isArray(reaction.expressions)) {
    const exp = reaction.expressions[Math.floor(Math.random() * reaction.expressions.length)];
    if (exp) {
      try { currentLive2DModel.expression(exp); } catch (e) {}
    }
  } else if (reaction.expression !== undefined) {
    try { currentLive2DModel.expression(reaction.expression); } catch (e) {}
  }

  if (reaction.subtitles && reaction.subtitles.length > 0) {
    const randomSub = reaction.subtitles[Math.floor(Math.random() * reaction.subtitles.length)];
    showSubtitle(randomSub, state === "talking" ? 4500 : 3200);
  }
}

function renderState(state) {
  currentState = state;
  const charDef = config?.characters[currentKey];
  if (!charDef) return;

  console.log("[ScreenMate] state:", state);
  if (charDef.type === "live2d") {
    applyLive2DState(charDef, state);
  } else {
    apply2DImageState(charDef, state);
  }
}

function queueState(targetState) {
  const now = Date.now();

  if (targetState === "thinking" || targetState === "busy") {
    stateLockUntil = now + 1500;
    if (pendingTimeout) {
      clearTimeout(pendingTimeout);
      pendingTimeout = null;
    }
    pendingState = null;
    renderState(targetState);
    return;
  }

  if (now < stateLockUntil) {
    pendingState = targetState;
    if (!pendingTimeout) {
      const delay = stateLockUntil - now;
      pendingTimeout = setTimeout(() => {
        pendingTimeout = null;
        if (pendingState) {
          const next = pendingState;
          pendingState = null;
          renderState(next);
        }
      }, delay);
    }
    return;
  }

  renderState(targetState);
}

function fitModelToCanvas() {
  if (!currentLive2DModel || !canvas) return;
  const model = currentLive2DModel;
  model.scale.set(1);
  const naturalWidth = model.width || 500;
  const naturalHeight = model.height || 600;

  // 100% 기준 캐릭터 기본 비율 (창 440x640의 약 72% 차지)
  const baseScale = Math.min(
    (canvas.width * 0.72) / naturalWidth,
    (canvas.height * 0.72) / naturalHeight
  );

  const finalScale = baseScale * currentScale;
  model.scale.set(finalScale);
  model.x = (canvas.width - naturalWidth * finalScale) / 2;
  model.y = canvas.height - naturalHeight * finalScale - 16;
}

async function setScale(scale, notify = true) {
  currentScale = Math.min(Math.max(scale, 0.7), 1.5);
  localStorage.setItem("screenmate_scale", currentScale.toString());

  // 윈도우 창 크기는 440x640으로 넉넉하게 고정하고, 캐릭터 모델만 내부에서 배율 조절
  if (currentLive2DModel) {
    fitModelToCanvas();
  }

  if (imgEl) {
    imgEl.style.transform = `scale(${currentScale})`;
    imgEl.style.transformOrigin = "bottom center";
  }

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

async function loadLive2D(charDef) {
  canvas.classList.remove("hidden");
  imgEl.classList.add("hidden");

  if (!pixiApp) {
    pixiApp = new PIXI.Application({
      view: canvas,
      width: window.innerWidth,
      height: window.innerHeight,
      backgroundAlpha: 0,
      autoDensity: true,
      resolution: window.devicePixelRatio || 1,
    });
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
    renderState(currentState);
  } catch (err) {
    console.error("Live2D load failed:", err);
    showSubtitle("Live2D 로드 실패!");
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

  renderState(currentState);
}

async function switchCharacter(key) {
  if (!config || !config.characters[key]) return;
  currentKey = key;
  const charDef = config.characters[key];
  charNameEl.textContent = charDef.name || key;

  if (charDef.type === "live2d") {
    await loadLive2D(charDef);
  } else {
    load2DImage(charDef);
  }
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
      (canvas.width * 0.72) / naturalWidth,
      (canvas.height * 0.72) / naturalHeight
    );
    const finalScale = baseScale * currentScale;
    const charW = naturalWidth * finalScale;
    const charH = naturalHeight * finalScale;

    const charLeft = (canvas.clientWidth - charW) / 2 + charW * 0.12;
    const charRight = (canvas.clientWidth + charW) / 2 - charW * 0.12;
    const charTop = canvas.clientHeight - charH - 16 + charH * 0.12;
    const charBottom = canvas.clientHeight - 16;
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
  renderState("click_react");

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
  }
});

listen("state-change", (event) => {
  console.log("[state-change event]", event.payload.state);
  queueState(event.payload.state);
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
  } catch (e) {
    console.error("Init failed:", e);
  }
}

init();

