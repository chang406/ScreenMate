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

const BASE_WIDTH = 320;
const BASE_HEIGHT = 520;

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

async function setScale(scale, notify = true) {
  currentScale = Math.min(Math.max(scale, 0.7), 1.5);
  localStorage.setItem("screenmate_scale", currentScale.toString());

  const targetW = Math.round(BASE_WIDTH * currentScale);
  const targetH = Math.round(BASE_HEIGHT * currentScale);

  try {
    await invoke("set_window_size", { width: targetW, height: targetH });
  } catch (e) {
    console.error("set_window_size failed:", e);
  }

  if (pixiApp) {
    pixiApp.renderer.resize(window.innerWidth, window.innerHeight);
    fitModelToCanvas();
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

function fitModelToCanvas() {
  if (!currentLive2DModel || !canvas) return;
  const model = currentLive2DModel;
  model.scale.set(1);
  const naturalWidth = model.width || 500;
  const naturalHeight = model.height || 600;

  const scale = Math.min(
    (canvas.width * 0.85) / naturalWidth,
    (canvas.height * 0.72) / naturalHeight
  );
  model.scale.set(scale);
  model.x = (canvas.width - naturalWidth * scale) / 2;
  model.y = canvas.height - naturalHeight * scale - 12;
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

function cycleNextCharacter() {
  if (!config) return;
  const keys = Object.keys(config.characters);
  const nextIdx = (keys.indexOf(currentKey) + 1) % keys.length;
  switchCharacter(keys[nextIdx]);
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
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    const left = w * 0.15;
    const right = w * 0.85;
    const top = h * 0.18;
    const bottom = h * 0.98;
    return x >= left && x <= right && y >= top && y <= bottom;
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
  const menuWidth = 200;
  const menuHeight = 180;
  let posX = e.clientX;
  let posY = e.clientY;

  if (posX + menuWidth > window.innerWidth) {
    posX = window.innerWidth - menuWidth - 10;
  }
  if (posY + menuHeight > window.innerHeight) {
    posY = window.innerHeight - menuHeight - 10;
  }

  contextMenu.style.left = `${posX}px`;
  contextMenu.style.top = `${posY}px`;
  contextMenu.classList.remove("hidden");
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
  cycleNextCharacter();
});

menuExitBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  invoke("exit_app");
});

switchBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  cycleNextCharacter();
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

async function init() {
  try {
    const res = await fetch("characters.json");
    config = await res.json();
    await switchCharacter(config.activeCharacter || "mao");
    setScale(currentScale, false);
    updateAlwaysOnTopUI();
  } catch (e) {
    console.error("Init failed:", e);
  }
}

init();
