const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// Elements
const tabBtns = document.querySelectorAll(".tab-btn");
const tabPanes = document.querySelectorAll(".tab-pane");
const toastEl = document.getElementById("toast");
const charGrid = document.getElementById("character-grid");
const refreshGalleryBtn = document.getElementById("refresh-gallery-btn");
const deleteModeBtn = document.getElementById("delete-mode-btn");
let isDeleteMode = false;

// 2D Form Elements
const form2D = document.getElementById("form-add-2d");
const input2DId = document.getElementById("2d-id");
const input2DName = document.getElementById("2d-name");
const quickModeToggle = document.getElementById("2d-quick-mode");
const slotThinking = document.getElementById("slot-thinking");
const slotTalking = document.getElementById("slot-talking");

const fileIdle = document.getElementById("file-idle");
const fileThinking = document.getElementById("file-thinking");
const fileTalking = document.getElementById("file-talking");

const previewIdle = document.getElementById("preview-idle");
const previewThinking = document.getElementById("preview-thinking");
const previewTalking = document.getElementById("preview-talking");

const subsIdle = document.getElementById("subs-idle");
const subsThinking = document.getElementById("subs-thinking");
const subsTalking = document.getElementById("subs-talking");

// Live2D Form Elements
const formLive2D = document.getElementById("form-add-live2d");
const inputL2DId = document.getElementById("l2d-id");
const inputL2DName = document.getElementById("l2d-name");
const inputL2DFolder = document.getElementById("l2d-folder");
const btnAnalyzeL2D = document.getElementById("btn-analyze-l2d");
const btnSaveLive2D = document.getElementById("btn-save-live2d");
const l2dMappingSection = document.getElementById("l2d-mapping-section");
const detectedModelFile = document.getElementById("detected-model-file");
const detectedMotionCount = document.getElementById("detected-motion-count");

const mapSelects = {
  idle: document.getElementById("map-idle"),
  thinking: document.getElementById("map-thinking"),
  busy: document.getElementById("map-busy"),
  talking: document.getElementById("map-talking"),
  error: document.getElementById("map-error"),
  click_react: document.getElementById("map-click_react"),
  sleeping: document.getElementById("map-sleeping"),
};

let raw2DFiles = {
  idle: null,
  thinking: null,
  talking: null,
};

let currentParsedLive2D = null;
let toastTimeout = null;

// ==========================================================================
// Toast Helper
// ==========================================================================
function showToast(message, isError = false) {
  toastEl.textContent = message;
  toastEl.className = isError ? "toast error" : "toast";
  
  if (toastTimeout) clearTimeout(toastTimeout);
  toastTimeout = setTimeout(() => {
    toastEl.classList.add("hidden");
  }, 3000);
}

// ==========================================================================
// Tab Switching
// ==========================================================================
function switchTab(tabId) {
  tabBtns.forEach((btn) => {
    if (btn.dataset.tab === tabId) {
      btn.classList.add("active");
    } else {
      btn.classList.remove("active");
    }
  });

  tabPanes.forEach((pane) => {
    if (pane.id === `tab-${tabId}`) {
      pane.classList.add("active");
    } else {
      pane.classList.remove("active");
    }
  });

  if (tabId === "gallery") {
    loadGallery();
  } else if (tabId === "bubble") {
    loadBubbleSettingsTab();
  }
}

tabBtns.forEach((btn) => {
  btn.addEventListener("click", () => switchTab(btn.dataset.tab));
});

// ==========================================================================
// Tab 1: Character Gallery
// ==========================================================================
async function loadGallery() {
  try {
    const config = await invoke("get_character_configs");
    const activeKey = config.activeCharacter;
    const characters = config.characters || {};

    charGrid.innerHTML = "";

    // 삭제 모드 클래스 동기화
    if (isDeleteMode) {
      charGrid.classList.add("delete-mode");
    } else {
      charGrid.classList.remove("delete-mode");
    }

    for (const [key, charDef] of Object.entries(characters)) {
      const isCurrentActive = key === activeKey;
      const isPreset = key === "mao" || key === "pixel_cat" || key === "wanko";

      const card = document.createElement("div");
      let cardClass = "char-card";
      if (isCurrentActive) cardClass += " active";
      if (isPreset) cardClass += " is-preset";
      card.className = cardClass;

      // Thumbnail
      let thumbHtml = "";
      if (charDef.type === "live2d") {
        let icon = "🪄";
        if (key === "wanko") icon = "🐶";
        if (key === "huohuo") icon = "👻";
        thumbHtml = `<div class="card-thumb"><span class="live2d-avatar">${icon}</span></div>`;
      } else {
        const firstSrc = charDef.states?.idle?.src || Object.values(charDef.states || {})[0]?.src || "";
        thumbHtml = `<div class="card-thumb"><img src="${firstSrc}" alt="${charDef.name}" /></div>`;
      }

      let typeBadgeClass = "badge-image";
      let typeLabel = "2D Image";

      if (charDef.type === "live2d") {
        const is4K = charDef.is4k || (charDef.name && charDef.name.includes("4K")) || key === "mao";
        if (is4K) {
          typeBadgeClass = "badge-live2d-4k";
          typeLabel = "Live2D 4K";
        } else {
          typeBadgeClass = "badge-live2d";
          typeLabel = "Live2D";
        }
      }

      let actionsHtml = "";
      if (!isDeleteMode) {
        // 일반 모드: 모든 카드가 깔끔하게 1개의 버튼만 표시 (삭제 버튼 노출 X)
        actionsHtml = isCurrentActive
          ? '<button class="btn-primary btn-sm flex-1" disabled>현재 적용됨</button>'
          : `<button class="btn-primary btn-sm flex-1 select-char-btn" data-id="${key}">선택하여 사용</button>`;
      } else {
        // 삭제 모드: 프리셋은 비활성화, 커스텀 캐릭터는 삭제 실행 버튼 표시
        if (isPreset) {
          actionsHtml = '<button class="btn-sm flex-1 delete-overlay-btn preset-disabled" disabled>🔒 기본 프리셋 (삭제 불가)</button>';
        } else {
          actionsHtml = `<button class="btn-sm flex-1 delete-overlay-btn delete-char-btn" data-id="${key}" data-name="${charDef.name}">🗑️ 이 캐릭터 삭제</button>`;
        }
      }

      card.innerHTML = `
        ${isCurrentActive ? '<span class="active-pill">✓ 사용 중</span>' : ""}
        <div class="card-top">
          ${thumbHtml}
          <div class="card-info">
            <div class="card-name" title="${charDef.name}">${charDef.name}</div>
            <span class="card-type-badge ${typeBadgeClass}">${typeLabel}</span>
          </div>
        </div>
        <div class="card-actions">
          ${actionsHtml}
        </div>
      `;

      charGrid.appendChild(card);
    }

    // Attach listeners for select
    document.querySelectorAll(".select-char-btn").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const id = btn.dataset.id;
        try {
          await invoke("set_active_character", { id });
          showToast(`"${characters[id]?.name || id}" 캐릭터로 변경되었습니다! ✨`);
          loadGallery();
        } catch (err) {
          showToast(`캐릭터 변경 실패: ${err}`, true);
        }
      });
    });

    // Attach listeners for delete
    document.querySelectorAll(".delete-char-btn").forEach((btn) => {
      btn.addEventListener("click", async (e) => {
        e.stopPropagation();
        const id = btn.dataset.id;
        const name = btn.dataset.name;
        if (confirm(`정말로 "${name}" 캐릭터를 완전히 삭제하시겠습니까?\n(해당 캐릭터의 설정 및 파일이 삭제됩니다)`)) {
          try {
            await invoke("delete_custom_character", { id });
            showToast(`"${name}" 캐릭터가 삭제되었습니다.`);
            await loadGallery();
          } catch (err) {
            showToast(`삭제 실패: ${err}`, true);
          }
        }
      });
    });
  } catch (err) {
    console.error("loadGallery error:", err);
    showToast("캐릭터 목록을 불러오지 못했습니다.", true);
  }
}

deleteModeBtn.addEventListener("click", () => {
  isDeleteMode = !isDeleteMode;
  if (isDeleteMode) {
    deleteModeBtn.textContent = "❌ 삭제 모드 종료";
    deleteModeBtn.classList.add("btn-delete-mode-active");
    showToast("삭제할 커스텀 캐릭터의 [🗑️ 이 캐릭터 삭제] 버튼을 눌러주세요.");
  } else {
    deleteModeBtn.textContent = "🗑️ 삭제 모드";
    deleteModeBtn.classList.remove("btn-delete-mode-active");
  }
  loadGallery();
});

refreshGalleryBtn.addEventListener("click", loadGallery);

// ==========================================================================
// Tab 2: 2D Image Wizard
// ==========================================================================
function setupDropZone(fileInput, previewImg, slotKey) {
  const dropZone = fileInput.closest(".drop-zone");

  function handleFile(file) {
    if (!file || !file.type.startsWith("image/")) {
      showToast("이미지 파일(PNG, GIF, SVG, JPG, WebP)만 지원합니다.", true);
      return;
    }

    const reader = new FileReader();
    reader.onload = (e) => {
      const dataUrl = e.target.result;
      raw2DFiles[slotKey] = dataUrl;
      previewImg.src = dataUrl;
      previewImg.classList.remove("hidden");
      dropZone.querySelector(".drop-placeholder").style.display = "none";
    };
    reader.readAsDataURL(file);
  }

  fileInput.addEventListener("change", (e) => {
    if (e.target.files && e.target.files[0]) {
      handleFile(e.target.files[0]);
    }
  });

  dropZone.addEventListener("dragover", (e) => {
    e.preventDefault();
    dropZone.classList.add("dragover");
  });

  dropZone.addEventListener("dragleave", () => {
    dropZone.classList.remove("dragover");
  });

  dropZone.addEventListener("drop", (e) => {
    e.preventDefault();
    dropZone.classList.remove("dragover");
    if (e.dataTransfer.files && e.dataTransfer.files[0]) {
      handleFile(e.dataTransfer.files[0]);
    }
  });
}

setupDropZone(fileIdle, previewIdle, "idle");
setupDropZone(fileThinking, previewThinking, "thinking");
setupDropZone(fileTalking, previewTalking, "talking");

quickModeToggle.addEventListener("change", () => {
  const isQuick = quickModeToggle.checked;
  if (isQuick) {
    slotThinking.classList.add("disabled");
    slotTalking.classList.add("disabled");
  } else {
    slotThinking.classList.remove("disabled");
    slotTalking.classList.remove("disabled");
  }
});

form2D.addEventListener("submit", async (e) => {
  e.preventDefault();

  const id = input2DId.value.trim().toLowerCase();
  const name = input2DName.value.trim();
  const isQuick = quickModeToggle.checked;

  if (!raw2DFiles.idle) {
    showToast("대기(IDLE) 이미지는 필수입니다!", true);
    return;
  }

  const filesPayload = {
    idle: raw2DFiles.idle,
    thinking: isQuick ? raw2DFiles.idle : (raw2DFiles.thinking || raw2DFiles.idle),
    talking: isQuick ? raw2DFiles.idle : (raw2DFiles.talking || raw2DFiles.idle),
  };

  const parseSubtitles = (input) => {
    return input.value
      .split(",")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
  };

  const subtitlesPayload = {
    idle: parseSubtitles(subsIdle),
    thinking: parseSubtitles(subsThinking),
    talking: parseSubtitles(subsTalking),
  };

  try {
    showToast("2D 캐릭터 생성 중...");
    await invoke("save_2d_character", {
      payload: {
        id,
        name,
        files: filesPayload,
        subtitles: subtitlesPayload,
      },
    });

    // Automatically switch to the newly created character
    await invoke("set_active_character", { id });

    showToast(`🎉 "${name}" 캐릭터가 생성 및 적용되었습니다!`);
    form2D.reset();
    previewIdle.classList.add("hidden");
    previewThinking.classList.add("hidden");
    previewTalking.classList.add("hidden");
    document.querySelectorAll(".drop-placeholder").forEach((el) => (el.style.display = "flex"));
    raw2DFiles = { idle: null, thinking: null, talking: null };

    switchTab("gallery");
  } catch (err) {
    showToast(`저장 실패: ${err}`, true);
  }
});

// ==========================================================================
// Tab 3: Live2D Smart Auto-Detect & Importer
// ==========================================================================
btnAnalyzeL2D.addEventListener("click", async () => {
  const folderPath = inputL2DFolder.value.trim();
  if (!folderPath) {
    showToast("Live2D 모델 폴더 경로를 입력해 주세요.", true);
    return;
  }

  try {
    btnAnalyzeL2D.textContent = "분석 중...";
    btnAnalyzeL2D.disabled = true;

    const result = await invoke("parse_live2d_folder", { folderPath });
    currentParsedLive2D = result;

    detectedModelFile.textContent = result.model_filename;
    detectedMotionCount.textContent = result.motion_groups.length;

    // Populate Select Options
    for (const [state, selectEl] of Object.entries(mapSelects)) {
      selectEl.innerHTML = "";
      result.motion_groups.forEach((group) => {
        const opt = document.createElement("option");
        opt.value = group;
        opt.textContent = group;
        selectEl.appendChild(opt);
      });

      // Set suggested mapping
      const suggested = result.suggested_mapping[state];
      if (suggested && result.motion_groups.includes(suggested)) {
        selectEl.value = suggested;
      }
    }

    l2dMappingSection.classList.remove("hidden");
    btnSaveLive2D.disabled = false;
    if (result.is_4k) {
      showToast("✨ 4K 초고화질(4096px) 텍스처 및 모션 자동 감지 완료!");
    } else {
      showToast("✨ Live2D 모델 및 모션 자동 감지 완료!");
    }
  } catch (err) {
    showToast(`분석 실패: ${err}`, true);
    l2dMappingSection.classList.add("hidden");
    btnSaveLive2D.disabled = true;
  } finally {
    btnAnalyzeL2D.textContent = "🔍 스마트 분석";
    btnAnalyzeL2D.disabled = false;
  }
});

formLive2D.addEventListener("submit", async (e) => {
  e.preventDefault();
  if (!currentParsedLive2D) {
    showToast("먼저 스마트 분석을 완료해 주세요.", true);
    return;
  }

  const id = inputL2DId.value.trim().toLowerCase();
  const name = inputL2DName.value.trim();

  const reactions = {
    idle: {
      motionGroup: mapSelects.idle.value,
      motionIndex: [0],
      subtitles: ["대기 중이에요~", "쉬는 중...", "무엇이든 물어보세요!"],
    },
    thinking: {
      motionGroup: mapSelects.thinking.value,
      motionIndex: [0],
      subtitles: ["어디 보자...", "생각 중이에요!", "질문 분석 중..."],
    },
    busy: {
      motionGroup: mapSelects.busy.value,
      motionIndex: [0],
      subtitles: ["열심히 코딩 중이에요! 🪄", "작업 실행 중!"],
    },
    talking: {
      motionGroup: mapSelects.talking.value,
      motionIndex: [0],
      subtitles: ["작업 완료! 확인해 주세요! ✨", "다 만들었어요!"],
    },
    error: {
      motionGroup: mapSelects.error.value,
      motionIndex: [0],
      subtitles: ["어라? 에러가 발생했어요! 💦", "에러 확인 부탁드려요!"],
    },
    click_react: {
      motionGroup: mapSelects.click_react.value,
      motionIndex: [0],
      subtitles: ["화이팅이에요! 힘내세요! 💕", "오늘도 멋진 코딩 해봐요!"],
    },
    sleeping: {
      motionGroup: mapSelects.sleeping.value,
      motionIndex: [0],
      subtitles: ["Zzz... (새근새근)", "쿨쿨... 💤"],
    },
  };

  try {
    showToast("Live2D 모델 복사 및 등록 중...");
    await invoke("save_live2d_character", {
      payload: {
        id,
        name,
        sourceFolder: currentParsedLive2D.model_folder,
        modelFilename: currentParsedLive2D.model_filename,
        reactions,
        is_4k: currentParsedLive2D.is_4k || false,
      },
    });

    // Switch to new character
    await invoke("set_active_character", { id });

    showToast(`🎉 "${name}" Live2D 캐릭터가 등록되었습니다!`);
    formLive2D.reset();
    l2dMappingSection.classList.add("hidden");
    btnSaveLive2D.disabled = true;
    currentParsedLive2D = null;

    switchTab("gallery");
  } catch (err) {
    showToast(`Live2D 등록 실패: ${err}`, true);
  }
});

// ==========================================================================
// Tab 4: Speech Bubble Settings
// ==========================================================================
const bubbleTargetSelect = document.getElementById("bubble-target-select");
const mockBubble = document.getElementById("mock-bubble");

const formBubble = document.getElementById("form-bubble-settings");
const bubbleAutoTrack = document.getElementById("bubble-auto-track");
const bubbleOffsetY = document.getElementById("bubble-offset-y");
const valBubbleOffsetY = document.getElementById("val-bubble-offset-y");
const bubbleOffsetX = document.getElementById("bubble-offset-x");
const valBubbleOffsetX = document.getElementById("val-bubble-offset-x");
const bubbleDuration = document.getElementById("bubble-duration");
const valBubbleDuration = document.getElementById("val-bubble-duration");
const bubbleFontSize = document.getElementById("bubble-font-size");
const valBubbleFontSize = document.getElementById("val-bubble-font-size");
const bubbleBgColor = document.getElementById("bubble-bg-color");
const bubbleBgColorText = document.getElementById("bubble-bg-color-text");
const bubbleBgOpacity = document.getElementById("bubble-bg-opacity");
const valBubbleBgOpacity = document.getElementById("val-bubble-bg-opacity");
const bubbleBorderColor = document.getElementById("bubble-border-color");
const bubbleBorderColorText = document.getElementById("bubble-border-color-text");
const bubbleBorderRadius = document.getElementById("bubble-border-radius");
const valBubbleBorderRadius = document.getElementById("val-bubble-border-radius");
const bubbleHasTail = document.getElementById("bubble-has-tail");

const btnTestBubble = document.getElementById("btn-test-bubble");
const btnResetBubble = document.getElementById("btn-reset-bubble");

let cachedBubbleConfig = null;

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

function updateMockBubble() {
  if (!mockBubble) return;

  const bgRgba = hexToRgba(bubbleBgColor.value, parseFloat(bubbleBgOpacity.value));
  const borderColor = bubbleBorderColor.value || "rgba(255, 255, 255, 0.25)";
  const fontSize = `${bubbleFontSize.value}px`;
  const radius = `${bubbleBorderRadius.value}px`;
  const hasTail = bubbleHasTail.checked;
  const offsetY = parseInt(bubbleOffsetY.value, 10) || 0;
  const offsetX = parseInt(bubbleOffsetX.value, 10) || 0;

  mockBubble.style.background = bgRgba;
  mockBubble.style.borderColor = borderColor;
  mockBubble.style.fontSize = fontSize;
  mockBubble.style.borderRadius = radius;
  mockBubble.style.transform = `translate(${offsetX}px, ${-offsetY}px)`;

  let tailStyle = document.getElementById("mock-tail-dynamic-style");
  if (!tailStyle) {
    tailStyle = document.createElement("style");
    tailStyle.id = "mock-tail-dynamic-style";
    document.head.appendChild(tailStyle);
  }
  tailStyle.textContent = `
    #mock-bubble::after {
      border-color: ${bgRgba} transparent !important;
      display: ${hasTail ? "block" : "none"} !important;
    }
  `;
}

function syncSliderLabels() {
  const offY = parseInt(bubbleOffsetY.value, 10) || 0;
  valBubbleOffsetY.textContent = offY > 0 ? `+${offY}px (위로)` : (offY < 0 ? `${offY}px (아래로)` : "0px (기본)");

  const offX = parseInt(bubbleOffsetX.value, 10) || 0;
  valBubbleOffsetX.textContent = offX > 0 ? `+${offX}px (우측)` : (offX < 0 ? `${offX}px (좌측)` : "0px (중앙)");

  valBubbleDuration.textContent = `${parseFloat(bubbleDuration.value).toFixed(1)}초`;
  valBubbleFontSize.textContent = `${bubbleFontSize.value}px`;
  valBubbleBgOpacity.textContent = `${Math.round(parseFloat(bubbleBgOpacity.value) * 100)}%`;
  valBubbleBorderRadius.textContent = `${bubbleBorderRadius.value}px`;
}

async function loadBubbleSettingsTab() {
  try {
    cachedBubbleConfig = await invoke("get_character_configs");
    const characters = cachedBubbleConfig.characters || {};
    const activeKey = cachedBubbleConfig.activeCharacter;

    const prevSelected = bubbleTargetSelect.value || "global";
    bubbleTargetSelect.innerHTML = `<option value="global">🌐 전역 기본값 (모든 캐릭터 공통)</option>`;

    for (const [key, charDef] of Object.entries(characters)) {
      const opt = document.createElement("option");
      opt.value = key;
      opt.textContent = `👤 ${charDef.name || key}${key === activeKey ? " (현재 사용 중)" : ""}`;
      bubbleTargetSelect.appendChild(opt);
    }

    if (characters[prevSelected] || prevSelected === "global") {
      bubbleTargetSelect.value = prevSelected;
    } else {
      bubbleTargetSelect.value = "global";
    }

    loadBubbleTargetSettings(bubbleTargetSelect.value);
  } catch (err) {
    console.error("loadBubbleSettingsTab failed:", err);
  }
}

const FACTORY_DEFAULT_BUBBLE_SETTINGS = {
  autoTrack: true,
  offsetX: 0,
  offsetY: 0,
  duration: 3.5,
  fontSize: 13,
  bgColor: "#0f0f14",
  bgOpacity: 0.92,
  borderColor: "#404050",
  borderRadius: 14,
  hasTail: true,
};

function applySettingsToForm(settings) {
  bubbleAutoTrack.checked = settings.autoTrack !== false;
  bubbleOffsetY.value = settings.offsetY ?? 0;
  bubbleOffsetX.value = settings.offsetX ?? 0;
  bubbleDuration.value = settings.duration ?? 3.5;
  bubbleFontSize.value = settings.fontSize ?? 13;
  bubbleBgColor.value = settings.bgColor || "#0f0f14";
  bubbleBgColorText.value = settings.bgColor || "#0f0f14";
  bubbleBgOpacity.value = settings.bgOpacity ?? 0.92;
  bubbleBorderColor.value = settings.borderColor || "#404050";
  bubbleBorderColorText.value = settings.borderColor || "#404050";
  bubbleBorderRadius.value = settings.borderRadius ?? 14;
  bubbleHasTail.checked = settings.hasTail !== false;

  syncSliderLabels();
  updateMockBubble();
}

function loadBubbleTargetSettings(targetId) {
  if (!cachedBubbleConfig) return;

  const global = cachedBubbleConfig.globalBubbleSettings || FACTORY_DEFAULT_BUBBLE_SETTINGS;

  if (targetId === "global") {
    applySettingsToForm(global);
  } else {
    const charDef = cachedBubbleConfig.characters?.[targetId];
    const custom = charDef?.bubbleSettings;
    const activeSettings = (custom && custom.useCustom !== false) ? { ...global, ...custom } : global;
    applySettingsToForm(activeSettings);
  }
}

// Event Listeners for Bubble Form Controls
bubbleTargetSelect.addEventListener("change", () => {
  loadBubbleTargetSettings(bubbleTargetSelect.value);
});

// Slider input sync
[
  [bubbleOffsetY, valBubbleOffsetY, (v) => {
    const val = parseInt(v, 10) || 0;
    return val > 0 ? `+${val}px (위로)` : (val < 0 ? `${val}px (아래로)` : "0px (기본)");
  }],
  [bubbleOffsetX, valBubbleOffsetX, (v) => {
    const val = parseInt(v, 10) || 0;
    return val > 0 ? `+${val}px (우측)` : (val < 0 ? `${val}px (좌측)` : "0px (중앙)");
  }],
  [bubbleDuration, valBubbleDuration, (v) => `${parseFloat(v).toFixed(1)}초`],
  [bubbleFontSize, valBubbleFontSize, (v) => `${v}px`],
  [bubbleBgOpacity, valBubbleBgOpacity, (v) => `${Math.round(parseFloat(v) * 100)}%`],
  [bubbleBorderRadius, valBubbleBorderRadius, (v) => `${v}px`],
].forEach(([input, label, fmt]) => {
  input.addEventListener("input", () => {
    label.textContent = fmt(input.value);
    updateMockBubble();
  });
});

bubbleAutoTrack.addEventListener("change", updateMockBubble);
bubbleHasTail.addEventListener("change", updateMockBubble);

// Color input sync
bubbleBgColor.addEventListener("input", () => {
  bubbleBgColorText.value = bubbleBgColor.value;
  updateMockBubble();
});
bubbleBgColorText.addEventListener("input", () => {
  if (/^#[0-9A-Fa-f]{6}$/.test(bubbleBgColorText.value)) {
    bubbleBgColor.value = bubbleBgColorText.value;
    updateMockBubble();
  }
});

bubbleBorderColor.addEventListener("input", () => {
  bubbleBorderColorText.value = bubbleBorderColor.value;
  updateMockBubble();
});
bubbleBorderColorText.addEventListener("input", () => {
  bubbleBorderColor.value = bubbleBorderColorText.value;
  updateMockBubble();
});

// Test Bubble Button
btnTestBubble.addEventListener("click", async () => {
  try {
    const currentFormSettings = {
      autoTrack: bubbleAutoTrack.checked,
      offsetY: parseInt(bubbleOffsetY.value, 10),
      offsetX: parseInt(bubbleOffsetX.value, 10),
      duration: parseFloat(bubbleDuration.value),
      fontSize: parseInt(bubbleFontSize.value, 10),
      bgColor: bubbleBgColorText.value,
      bgOpacity: parseFloat(bubbleBgOpacity.value),
      borderColor: bubbleBorderColorText.value,
      borderRadius: parseInt(bubbleBorderRadius.value, 10),
      hasTail: bubbleHasTail.checked,
    };

    await invoke("trigger_test_bubble", {
      payload: {
        text: "💬 말풍선 미리보기 테스트입니다! ✨",
        settings: currentFormSettings,
      },
    });
    showToast("오버레이 화면에 테스트 말풍선을 띄웠습니다!");
  } catch (err) {
    showToast(`테스트 실패: ${err}`, true);
  }
});

// Reset Button
btnResetBubble.addEventListener("click", () => {
  const targetId = bubbleTargetSelect.value;
  if (targetId === "global") {
    applySettingsToForm(FACTORY_DEFAULT_BUBBLE_SETTINGS);
    showToast("말풍선 표준 기본값(Factory Default)으로 초기화되었습니다.");
  } else {
    const global = cachedBubbleConfig?.globalBubbleSettings || FACTORY_DEFAULT_BUBBLE_SETTINGS;
    applySettingsToForm(global);
    showToast("전역 기본값으로 복귀되었습니다. [설정 저장]을 누르면 적용됩니다.");
  }
});

// Save Bubble Settings
formBubble.addEventListener("submit", async (e) => {
  e.preventDefault();
  const targetId = bubbleTargetSelect.value;

  const settings = {
    autoTrack: bubbleAutoTrack.checked,
    offsetY: parseInt(bubbleOffsetY.value, 10),
    offsetX: parseInt(bubbleOffsetX.value, 10),
    duration: parseFloat(bubbleDuration.value),
    fontSize: parseInt(bubbleFontSize.value, 10),
    bgColor: bubbleBgColorText.value,
    bgOpacity: parseFloat(bubbleBgOpacity.value),
    borderColor: bubbleBorderColorText.value,
    borderRadius: parseInt(bubbleBorderRadius.value, 10),
    hasTail: bubbleHasTail.checked,
  };

  if (targetId !== "global") {
    settings.useCustom = true;
  }

  try {
    showToast("말풍선 설정 저장 중...");
    await invoke("save_bubble_settings", {
      payload: {
        target_id: targetId,
        settings,
      },
    });

    cachedBubbleConfig = await invoke("get_character_configs");
    showToast("💾 말풍선 설정이 성공적으로 저장되었습니다!");
  } catch (err) {
    showToast(`설정 저장 실패: ${err}`, true);
  }
});

// ==========================================================================
// Real-time Event Listener & Lifecycle Sync
// ==========================================================================
function refreshActiveTab() {
  loadGallery();
  const activeTab = document.querySelector(".tab-btn.active")?.dataset.tab;
  if (activeTab === "bubble") {
    loadBubbleSettingsTab();
  }
}

listen("characters-updated", () => {
  refreshActiveTab();
});

listen("switch-character", () => {
  refreshActiveTab();
});

listen("bubble-settings-updated", () => {
  refreshActiveTab();
});

window.addEventListener("focus", () => {
  refreshActiveTab();
});

document.addEventListener("visibilitychange", () => {
  if (!document.hidden) {
    refreshActiveTab();
  }
});

// Initial load
loadGallery();
