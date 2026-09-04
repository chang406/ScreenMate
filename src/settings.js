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
let knownCharacterIds = new Set();

// 2D Form Elements
const form2D = document.getElementById("form-add-2d");
const input2DId = document.getElementById("2d-id");
const input2DName = document.getElementById("2d-name");
const status2DId = document.getElementById("2d-id-status");
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
const statusL2DId = document.getElementById("l2d-id-status");
const inputL2DFolder = document.getElementById("l2d-folder");
const btnBrowseL2D = document.getElementById("btn-browse-l2d");
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

const mapExpSelects = {
  idle: document.getElementById("map-exp-idle"),
  thinking: document.getElementById("map-exp-thinking"),
  busy: document.getElementById("map-exp-busy"),
  talking: document.getElementById("map-exp-talking"),
  error: document.getElementById("map-exp-error"),
  click_react: document.getElementById("map-exp-click_react"),
  sleeping: document.getElementById("map-exp-sleeping"),
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
  if (!toastEl) return;
  toastEl.textContent = message;
  toastEl.className = isError ? "toast error" : "toast";
  
  if (toastTimeout) clearTimeout(toastTimeout);
  toastTimeout = setTimeout(() => {
    toastEl.classList.add("hidden");
  }, 3000);
}

// ==========================================================================
// ID Auto-Sync & Duplicate Validation Helpers
// ==========================================================================
function slugifyName(name) {
  if (!name) return "";
  return name
    .toLowerCase()
    .replace(/[^a-zA-Z0-9_\-\s]/g, "")
    .trim()
    .replace(/[\s\-]+/g, "_");
}

function validateId(inputEl, statusEl) {
  if (!inputEl || !statusEl) return true;
  const id = inputEl.value.trim().toLowerCase();
  if (!id) {
    statusEl.className = "id-status-msg";
    statusEl.textContent = "";
    inputEl.classList.remove("has-warning", "has-success");
    return true;
  }

  if (knownCharacterIds.has(id)) {
    statusEl.className = "id-status-msg show warning";
    statusEl.textContent = "⚠️ 이미 등록된 ID입니다. 저장 시 기존 캐릭터가 덮어써집니다.";
    inputEl.classList.add("has-warning");
    inputEl.classList.remove("has-success");
    return false;
  } else {
    statusEl.className = "id-status-msg show success";
    statusEl.textContent = "✓ 사용 가능한 고유 ID입니다.";
    inputEl.classList.remove("has-warning");
    inputEl.classList.add("has-success");
    return true;
  }
}

function setupIdAutoSyncAndCheck(inputName, inputId, statusEl) {
  if (!inputName || !inputId || !statusEl) return;
  let userModifiedId = false;

  inputName.addEventListener("input", () => {
    if (!userModifiedId || !inputId.value.trim()) {
      const slug = slugifyName(inputName.value);
      if (slug) {
        inputId.value = slug;
      }
      validateId(inputId, statusEl);
    }
  });

  inputId.addEventListener("input", () => {
    userModifiedId = inputId.value.trim().length > 0;
    validateId(inputId, statusEl);
  });
}

setupIdAutoSyncAndCheck(input2DName, input2DId, status2DId);
setupIdAutoSyncAndCheck(inputL2DName, inputL2DId, statusL2DId);

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
  } else if (tabId === "agent") {
    loadAgentStatusTab();
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
    knownCharacterIds = new Set(Object.keys(characters));

    if (!charGrid) return;
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

if (deleteModeBtn) {
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
}

if (refreshGalleryBtn) {
  refreshGalleryBtn.addEventListener("click", loadGallery);
}

// ==========================================================================
// Tab 2: 2D Image Wizard
// ==========================================================================
function setupDropZone(fileInput, previewImg, slotKey) {
  if (!fileInput || !previewImg) return;
  const dropZone = fileInput.closest(".drop-zone");
  if (!dropZone) return;

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
      const placeholder = dropZone.querySelector(".drop-placeholder");
      if (placeholder) placeholder.style.display = "none";
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

if (quickModeToggle) {
  quickModeToggle.addEventListener("change", () => {
    const isQuick = quickModeToggle.checked;
    if (isQuick) {
      slotThinking?.classList.add("disabled");
      slotTalking?.classList.add("disabled");
    } else {
      slotThinking?.classList.remove("disabled");
      slotTalking?.classList.remove("disabled");
    }
  });
}

if (form2D) {
  form2D.addEventListener("submit", async (e) => {
    e.preventDefault();

    const id = input2DId.value.trim().toLowerCase();
    const name = input2DName.value.trim();
    const isQuick = quickModeToggle.checked;

    if (!raw2DFiles.idle) {
      showToast("대기(IDLE) 이미지는 필수입니다!", true);
      return;
    }

    // 중복 ID 덮어쓰기 확인
    if (knownCharacterIds.has(id)) {
      const ok = confirm(`"${id}" ID를 가진 캐릭터가 이미 존재합니다.\n해당 캐릭터의 설정과 에셋을 덮어쓰시겠습니까?`);
      if (!ok) return;
    }

    const filesPayload = {
      idle: raw2DFiles.idle,
      thinking: isQuick ? raw2DFiles.idle : (raw2DFiles.thinking || raw2DFiles.idle),
      talking: isQuick ? raw2DFiles.idle : (raw2DFiles.talking || raw2DFiles.idle),
    };

    const parseSubtitles = (input) => {
      if (!input) return [];
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
      previewIdle?.classList.add("hidden");
      previewThinking?.classList.add("hidden");
      previewTalking?.classList.add("hidden");
      document.querySelectorAll(".drop-placeholder").forEach((el) => (el.style.display = "flex"));
      raw2DFiles = { idle: null, thinking: null, talking: null };
      if (status2DId) {
        status2DId.className = "id-status-msg";
        status2DId.textContent = "";
      }
      input2DId?.classList.remove("has-warning", "has-success");

      switchTab("gallery");
    } catch (err) {
      showToast(`저장 실패: ${err}`, true);
    }
  });
}

// ==========================================================================
// Tab 3: Live2D Smart Auto-Detect & Importer
// ==========================================================================
if (btnBrowseL2D) {
  btnBrowseL2D.addEventListener("click", async () => {
    try {
      const selectedFolder = await invoke("pick_folder");
      if (selectedFolder) {
        inputL2DFolder.value = selectedFolder;

        // Auto-fill Name and ID if empty based on folder name
        const parts = selectedFolder.split(/[\\/]/).filter(Boolean);
        const folderName = parts[parts.length - 1] || "";
        if (inputL2DName && !inputL2DName.value.trim() && folderName) {
          inputL2DName.value = folderName;
        }
        if (inputL2DId && !inputL2DId.value.trim() && folderName) {
          inputL2DId.value = slugifyName(folderName);
          validateId(inputL2DId, statusL2DId);
        }

        showToast("📁 폴더 선택 완료! 스마트 분석을 시작합니다...");
        if (btnAnalyzeL2D) {
          btnAnalyzeL2D.click();
        }
      }
    } catch (err) {
      showToast(`폴더 선택 실패: ${err}`, true);
    }
  });
}

if (btnAnalyzeL2D) {
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

      if (detectedModelFile) detectedModelFile.textContent = result.model_filename;
      if (detectedMotionCount) {
        detectedMotionCount.textContent = `${result.motion_groups?.length || 0}개`;
      }
      const detectedExpCount = document.getElementById("detected-exp-count");
      if (detectedExpCount) {
        detectedExpCount.textContent = `${result.expressions?.length || 0}개`;
      }

      // Populate Motion Select Options
      for (const [state, selectEl] of Object.entries(mapSelects)) {
        if (!selectEl) continue;
        selectEl.innerHTML = "";

        const defaultOpt = document.createElement("option");
        defaultOpt.value = "";
        defaultOpt.textContent = result.motion_groups.length === 0
          ? "(기본 물리/호흡 대기 - 개별 모션 없음)"
          : "(모션 없음 / 기본 대기)";
        selectEl.appendChild(defaultOpt);

        result.motion_groups.forEach((group) => {
          const opt = document.createElement("option");
          opt.value = group;
          opt.textContent = group;
          selectEl.appendChild(opt);
        });

        // Set suggested motion mapping
        const suggested = result.suggested_mapping?.[state];
        if (suggested && result.motion_groups.includes(suggested)) {
          selectEl.value = suggested;
        } else {
          selectEl.value = "";
        }
      }

      // Populate Expression Select Options
      const expressionsList = result.expressions || [];
      for (const [state, expSelectEl] of Object.entries(mapExpSelects)) {
        if (!expSelectEl) continue;
        expSelectEl.innerHTML = "";

        const defaultExpOpt = document.createElement("option");
        defaultExpOpt.value = "";
        defaultExpOpt.textContent = expressionsList.length === 0
          ? "(기본 표정 - 개별 표정 없음)"
          : "(기본 표정 / 변경 없음)";
        expSelectEl.appendChild(defaultExpOpt);

        expressionsList.forEach((expName) => {
          const opt = document.createElement("option");
          opt.value = expName;
          opt.textContent = expName;
          expSelectEl.appendChild(opt);
        });

        // Set suggested expression mapping
        const suggestedExp = result.suggested_expressions?.[state];
        if (suggestedExp && expressionsList.includes(suggestedExp)) {
          expSelectEl.value = suggestedExp;
        } else {
          expSelectEl.value = "";
        }
      }

      if (l2dMappingSection) l2dMappingSection.classList.remove("hidden");
      if (btnSaveLive2D) btnSaveLive2D.disabled = false;
      showToast(`✨ 모션(${result.motion_groups.length}개) & 표정(${expressionsList.length}개) 스마트 자동 분석 완료!`);
    } catch (err) {
      showToast(`분석 실패: ${err}`, true);
      if (l2dMappingSection) l2dMappingSection.classList.add("hidden");
      if (btnSaveLive2D) btnSaveLive2D.disabled = true;
    } finally {
      btnAnalyzeL2D.textContent = "🔍 스마트 분석";
      btnAnalyzeL2D.disabled = false;
    }
  });
}

if (formLive2D) {
  formLive2D.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (!currentParsedLive2D) {
      showToast("먼저 스마트 분석을 완료해 주세요.", true);
      return;
    }

    const id = inputL2DId.value.trim().toLowerCase();
    const name = inputL2DName.value.trim();

    // 중복 ID 덮어쓰기 확인
    if (knownCharacterIds.has(id)) {
      const ok = confirm(`"${id}" ID를 가진 캐릭터가 이미 존재합니다.\n해당 캐릭터의 설정과 모델 파일을 덮어쓰시겠습니까?`);
      if (!ok) return;
    }

    const getReaction = (state, defaultSubtitles) => {
      const motion = mapSelects[state]?.value || "";
      const exp = mapExpSelects[state]?.value || "";
      return {
        motionGroup: motion,
        motionIndex: [0],
        expressions: exp ? [exp] : [],
        subtitles: defaultSubtitles,
      };
    };

    const reactions = {
      idle: getReaction("idle", ["대기 중이에요~", "쉬는 중...", "무엇이든 물어보세요!"]),
      thinking: getReaction("thinking", ["어디 보자...", "생각 중이에요!", "질문 분석 중..."]),
      busy: getReaction("busy", ["열심히 코딩 중이에요! 🪄", "작업 실행 중!"]),
      talking: getReaction("talking", ["작업 완료! 확인해 주세요! ✨", "다 만들었어요!"]),
      error: getReaction("error", ["어라? 에러가 발생했어요! 💦", "에러 확인 부탁드려요!"]),
      click_react: getReaction("click_react", ["화이팅이에요! 힘내세요! 💕", "오늘도 멋진 코딩 해봐요!"]),
      sleeping: getReaction("sleeping", ["Zzz... (새근새근)", "쿨쿨... 💤"]),
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
      if (l2dMappingSection) l2dMappingSection.classList.add("hidden");
      if (btnSaveLive2D) btnSaveLive2D.disabled = true;
      currentParsedLive2D = null;
      if (statusL2DId) {
        statusL2DId.className = "id-status-msg";
        statusL2DId.textContent = "";
      }
      inputL2DId?.classList.remove("has-warning", "has-success");

      switchTab("gallery");
    } catch (err) {
      showToast(`Live2D 등록 실패: ${err}`, true);
    }
  });
}

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

  const bgRgba = hexToRgba(bubbleBgColor?.value, parseFloat(bubbleBgOpacity?.value || "0.92"));
  const borderColor = bubbleBorderColor?.value || "rgba(255, 255, 255, 0.25)";
  const fontSize = `${bubbleFontSize?.value || 13}px`;
  const radius = `${bubbleBorderRadius?.value || 14}px`;
  const hasTail = bubbleHasTail ? bubbleHasTail.checked : true;
  const offsetY = parseInt(bubbleOffsetY?.value || "0", 10) || 0;
  const offsetX = parseInt(bubbleOffsetX?.value || "0", 10) || 0;

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
  if (bubbleOffsetY && valBubbleOffsetY) {
    const offY = parseInt(bubbleOffsetY.value, 10) || 0;
    valBubbleOffsetY.textContent = offY > 0 ? `+${offY}px (위로)` : (offY < 0 ? `${offY}px (아래로)` : "0px (기본)");
  }

  if (bubbleOffsetX && valBubbleOffsetX) {
    const offX = parseInt(bubbleOffsetX.value, 10) || 0;
    valBubbleOffsetX.textContent = offX > 0 ? `+${offX}px (우측)` : (offX < 0 ? `${offX}px (좌측)` : "0px (중앙)");
  }

  if (bubbleDuration && valBubbleDuration) valBubbleDuration.textContent = `${parseFloat(bubbleDuration.value).toFixed(1)}초`;
  if (bubbleFontSize && valBubbleFontSize) valBubbleFontSize.textContent = `${bubbleFontSize.value}px`;
  if (bubbleBgOpacity && valBubbleBgOpacity) valBubbleBgOpacity.textContent = `${Math.round(parseFloat(bubbleBgOpacity.value) * 100)}%`;
  if (bubbleBorderRadius && valBubbleBorderRadius) valBubbleBorderRadius.textContent = `${bubbleBorderRadius.value}px`;
}

async function loadBubbleSettingsTab() {
  try {
    cachedBubbleConfig = await invoke("get_character_configs");
    const characters = cachedBubbleConfig.characters || {};
    const activeKey = cachedBubbleConfig.activeCharacter;

    if (!bubbleTargetSelect) return;
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
  if (bubbleAutoTrack) bubbleAutoTrack.checked = settings.autoTrack !== false;
  if (bubbleOffsetY) bubbleOffsetY.value = settings.offsetY ?? 0;
  if (bubbleOffsetX) bubbleOffsetX.value = settings.offsetX ?? 0;
  if (bubbleDuration) bubbleDuration.value = settings.duration ?? 3.5;
  if (bubbleFontSize) bubbleFontSize.value = settings.fontSize ?? 13;
  if (bubbleBgColor) bubbleBgColor.value = settings.bgColor || "#0f0f14";
  if (bubbleBgColorText) bubbleBgColorText.value = settings.bgColor || "#0f0f14";
  if (bubbleBgOpacity) bubbleBgOpacity.value = settings.bgOpacity ?? 0.92;
  if (bubbleBorderColor) bubbleBorderColor.value = settings.borderColor || "#404050";
  if (bubbleBorderColorText) bubbleBorderColorText.value = settings.borderColor || "#404050";
  if (bubbleBorderRadius) bubbleBorderRadius.value = settings.borderRadius ?? 14;
  if (bubbleHasTail) bubbleHasTail.checked = settings.hasTail !== false;

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
if (bubbleTargetSelect) {
  bubbleTargetSelect.addEventListener("change", () => {
    loadBubbleTargetSettings(bubbleTargetSelect.value);
  });
}

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
  if (input && label) {
    input.addEventListener("input", () => {
      label.textContent = fmt(input.value);
      updateMockBubble();
    });
  }
});

if (bubbleAutoTrack) bubbleAutoTrack.addEventListener("change", updateMockBubble);
if (bubbleHasTail) bubbleHasTail.addEventListener("change", updateMockBubble);

// Color input sync
if (bubbleBgColor && bubbleBgColorText) {
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
}

if (bubbleBorderColor && bubbleBorderColorText) {
  bubbleBorderColor.addEventListener("input", () => {
    bubbleBorderColorText.value = bubbleBorderColor.value;
    updateMockBubble();
  });
  bubbleBorderColorText.addEventListener("input", () => {
    if (/^#[0-9A-Fa-f]{6}$/.test(bubbleBorderColorText.value)) {
      bubbleBorderColor.value = bubbleBorderColorText.value;
      updateMockBubble();
    }
  });
}

// Test Bubble Button
if (btnTestBubble) {
  btnTestBubble.addEventListener("click", async () => {
    try {
      const currentFormSettings = {
        autoTrack: bubbleAutoTrack ? bubbleAutoTrack.checked : true,
        offsetY: bubbleOffsetY ? parseInt(bubbleOffsetY.value, 10) : 0,
        offsetX: bubbleOffsetX ? parseInt(bubbleOffsetX.value, 10) : 0,
        duration: bubbleDuration ? parseFloat(bubbleDuration.value) : 3.5,
        fontSize: bubbleFontSize ? parseInt(bubbleFontSize.value, 10) : 13,
        bgColor: bubbleBgColorText ? bubbleBgColorText.value : "#0f0f14",
        bgOpacity: bubbleBgOpacity ? parseFloat(bubbleBgOpacity.value) : 0.92,
        borderColor: bubbleBorderColorText ? bubbleBorderColorText.value : "#404050",
        borderRadius: bubbleBorderRadius ? parseInt(bubbleBorderRadius.value, 10) : 14,
        hasTail: bubbleHasTail ? bubbleHasTail.checked : true,
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
}

// Reset Button
if (btnResetBubble) {
  btnResetBubble.addEventListener("click", () => {
    const targetId = bubbleTargetSelect ? bubbleTargetSelect.value : "global";
    if (targetId === "global") {
      applySettingsToForm(FACTORY_DEFAULT_BUBBLE_SETTINGS);
      showToast("말풍선 표준 기본값(Factory Default)으로 초기화되었습니다.");
    } else {
      const global = cachedBubbleConfig?.globalBubbleSettings || FACTORY_DEFAULT_BUBBLE_SETTINGS;
      applySettingsToForm(global);
      showToast("전역 기본값으로 복귀되었습니다. [설정 저장]을 누르면 적용됩니다.");
    }
  });
}

// Save Bubble Settings
if (formBubble) {
  formBubble.addEventListener("submit", async (e) => {
    e.preventDefault();
    const targetId = bubbleTargetSelect ? bubbleTargetSelect.value : "global";

    const settings = {
      autoTrack: bubbleAutoTrack ? bubbleAutoTrack.checked : true,
      offsetY: bubbleOffsetY ? parseInt(bubbleOffsetY.value, 10) : 0,
      offsetX: bubbleOffsetX ? parseInt(bubbleOffsetX.value, 10) : 0,
      duration: bubbleDuration ? parseFloat(bubbleDuration.value) : 3.5,
      fontSize: bubbleFontSize ? parseInt(bubbleFontSize.value, 10) : 13,
      bgColor: bubbleBgColorText ? bubbleBgColorText.value : "#0f0f14",
      bgOpacity: bubbleBgOpacity ? parseFloat(bubbleBgOpacity.value) : 0.92,
      borderColor: bubbleBorderColorText ? bubbleBorderColorText.value : "#404050",
      borderRadius: bubbleBorderRadius ? parseInt(bubbleBorderRadius.value, 10) : 14,
      hasTail: bubbleHasTail ? bubbleHasTail.checked : true,
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
}

// ==========================================================================
// Real-time Event Listener & Lifecycle Sync
// ==========================================================================
function refreshActiveTab() {
  loadGallery();
  const activeTab = document.querySelector(".tab-btn.active")?.dataset.tab;
  if (activeTab === "bubble") {
    loadBubbleSettingsTab();
  } else if (activeTab === "agent") {
    loadAgentStatusTab();
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

// ==========================================================================
// Tab 5: AI Agent Connection & Watch Paths
// ==========================================================================
const agentStatusDot = document.getElementById("agent-status-dot");
const agentStatusTitle = document.getElementById("agent-status-title");
const agentStatusDesc = document.getElementById("agent-status-desc");
const agentToolBadge = document.getElementById("agent-tool-badge");
const agentSessionFile = document.getElementById("agent-session-file");
const customPathInput = document.getElementById("custom-path-input");
const btnBrowseWatchPath = document.getElementById("btn-browse-watch-path");
const btnAddWatchPath = document.getElementById("btn-add-watch-path");
const customPathsList = document.getElementById("custom-paths-list");
const watchedPathsTbody = document.getElementById("watched-paths-tbody");
const simBtns = document.querySelectorAll(".btn-sim");

// 프라이버시 보호를 위해 로컬 사용자 홈 디렉토리(C:\Users\<username>)를 '~'로 안전하게 마스킹
function maskHomePath(pathStr) {
  if (!pathStr || typeof pathStr !== "string") return "-";
  let masked = pathStr
    .replace(/^[a-zA-Z]:[\\\/]Users[\\\/][^\\\/]+/i, "~")
    .replace(/^\/Users\/[^\/]+/i, "~")
    .replace(/^\/home\/[^\/]+/i, "~");

  return masked.replace(/\\/g, "/");
}

function getToolDisplayName(toolId) {
  switch (toolId) {
    case "antigravity":
      return "Antigravity (IDE / CLI)";
    case "claude":
      return "Claude Code";
    case "chatgpt_codex":
      return "ChatGPT / Codex";
    default:
      return toolId || "대기 중";
  }
}

function renderAgentStatusUI(status) {
  if (!status) return;

  if (agentStatusDot) {
    agentStatusDot.className = "status-dot";
    if (status.is_connected) {
      agentStatusDot.classList.add("status-connected");
    } else {
      agentStatusDot.classList.add("status-waiting");
    }
  }

  if (agentStatusTitle) {
    if (status.is_connected) {
      agentStatusTitle.textContent = `${getToolDisplayName(status.active_tool)} 연결됨 🟢`;
    } else {
      agentStatusTitle.textContent = "AI 에이전트 감시 대기 중...";
    }
  }

  if (agentStatusDesc) {
    if (status.is_connected) {
      agentStatusDesc.textContent = "대화 및 명령어 세션 로그를 실시간 추적하여 캐릭터가 반응하고 있습니다.";
    } else {
      agentStatusDesc.textContent = "감시 대상 폴더를 150ms 주기로 탐색하고 있습니다. AI 작업을 시작하면 자동 감지됩니다.";
    }
  }

  if (agentToolBadge) {
    agentToolBadge.textContent = status.is_connected ? getToolDisplayName(status.active_tool) : "대기 중";
  }

  if (agentSessionFile) {
    if (status.active_session_file) {
      agentSessionFile.textContent = maskHomePath(status.active_session_file);
      agentSessionFile.title = `실제 전체 경로: ${status.active_session_file}`;
    } else {
      agentSessionFile.textContent = "감지된 활성 세션 없음";
      agentSessionFile.removeAttribute("title");
    }
  }

  // 기본 및 등록된 경로 테이블 렌더링
  if (watchedPathsTbody && Array.isArray(status.watched_paths)) {
    watchedPathsTbody.innerHTML = status.watched_paths
      .map((item) => {
        const typeBadge =
          item.source === "custom"
            ? '<span class="badge-tag tag-custom">사용자 정의</span>'
            : item.source === "env"
              ? '<span class="badge-tag tag-env">환경 변수</span>'
              : '<span class="badge-tag tag-default">기본 경로</span>';

        const statusBadge = item.exists
          ? '<span class="badge-status-ok">✓ 감지됨</span>'
          : '<span class="badge-status-missing">✕ 미설치</span>';

        const maskedPath = maskHomePath(item.path);

        return `
          <tr>
            <td style="max-width: 280px; word-break: break-all; font-family: monospace;" title="실제 전체 경로: ${item.path}">${maskedPath}</td>
            <td>${typeBadge}</td>
            <td>${statusBadge}</td>
            <td>${item.session_count}개</td>
          </tr>
        `;
      })
      .join("");
  }
}

async function renderCustomPathsList() {
  if (!customPathsList) return;
  try {
    const paths = await invoke("get_custom_watch_paths");
    if (!paths || paths.length === 0) {
      customPathsList.innerHTML = '<div class="empty-paths-msg">등록된 사용자 정의 경로가 없습니다. (기본 표준 경로로 자동 감시 중)</div>';
      return;
    }

    customPathsList.innerHTML = paths
      .map(
        (p) => `
      <div class="custom-path-item" title="실제 전체 경로: ${p}">
        <span>📁 ${maskHomePath(p)}</span>
        <button type="button" class="btn-remove-path" data-path="${encodeURIComponent(p)}" title="경로 제거">✕ 삭제</button>
      </div>
    `
      )
      .join("");

    customPathsList.querySelectorAll(".btn-remove-path").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const rawPath = decodeURIComponent(btn.dataset.path);
        try {
          await invoke("remove_custom_watch_path", { path: rawPath });
          showToast("감시 경로가 제거되었습니다.");
          await loadAgentStatusTab();
        } catch (err) {
          showToast(`제거 실패: ${err}`, true);
        }
      });
    });
  } catch (err) {
    console.error("Failed to load custom watch paths:", err);
  }
}

async function loadAgentStatusTab() {
  try {
    const status = await invoke("get_agent_status");
    renderAgentStatusUI(status);
    await renderCustomPathsList();
  } catch (err) {
    console.error("Failed to load agent status:", err);
  }
}

// 브라우즈 버튼
if (btnBrowseWatchPath) {
  btnBrowseWatchPath.addEventListener("click", async () => {
    try {
      const selected = await invoke("pick_folder");
      if (selected && customPathInput) {
        customPathInput.value = selected;
      }
    } catch (err) {
      showToast(`폴더 선택 실패: ${err}`, true);
    }
  });
}

// 경로 추가 버튼
if (btnAddWatchPath) {
  btnAddWatchPath.addEventListener("click", async () => {
    if (!customPathInput) return;
    const path = customPathInput.value.trim();
    if (!path) {
      showToast("추가할 폴더 경로를 입력해주세요.", true);
      return;
    }

    try {
      await invoke("add_custom_watch_path", { path });
      customPathInput.value = "";
      showToast("📁 새 감시 경로가 성공적으로 등록되었습니다!");
      await loadAgentStatusTab();
    } catch (err) {
      showToast(`경로 추가 실패: ${err}`, true);
    }
  });
}

// 모의 시뮬레이터 재생 버튼들
simBtns.forEach((btn) => {
  btn.addEventListener("click", async () => {
    const simType = btn.dataset.sim;
    try {
      showToast(`🧪 [${getToolDisplayName(simType)}] 가상 세션을 재생합니다. 캐릭터 반응을 확인하세요!`);
      await invoke("replay_fixture_session", { agentId: simType });
    } catch (err) {
      showToast(`시뮬레이션 실패: ${err}`, true);
    }
  });
});

listen("agent-status-changed", (event) => {
  const activeTab = document.querySelector(".tab-btn.active")?.dataset.tab;
  if (activeTab === "agent") {
    renderAgentStatusUI(event.payload);
  }
});

// Initial load
loadGallery();
