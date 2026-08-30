const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// Elements
const tabBtns = document.querySelectorAll(".tab-btn");
const tabPanes = document.querySelectorAll(".tab-pane");
const toastEl = document.getElementById("toast");
const charGrid = document.getElementById("character-grid");
const refreshGalleryBtn = document.getElementById("refresh-gallery-btn");

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

    for (const [key, charDef] of Object.entries(characters)) {
      const isCurrentActive = key === activeKey;
      const isPreset = key === "mao" || key === "pixel_cat" || key === "wanko";

      const card = document.createElement("div");
      card.className = isCurrentActive ? "char-card active" : "char-card";

      // Thumbnail
      let thumbHtml = "";
      if (charDef.type === "live2d") {
        const icon = key === "wanko" ? "🐶" : "🪄";
        thumbHtml = `<div class="card-thumb"><span class="live2d-avatar">${icon}</span></div>`;
      } else {
        const firstSrc = charDef.states?.idle?.src || Object.values(charDef.states || {})[0]?.src || "";
        thumbHtml = `<div class="card-thumb"><img src="${firstSrc}" alt="${charDef.name}" /></div>`;
      }

      const typeBadgeClass = charDef.type === "live2d" ? "badge-live2d" : "badge-image";
      const typeLabel = charDef.type === "live2d" ? "Live2D 4K" : "2D Image";

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
          ${
            isCurrentActive
              ? '<button class="btn-primary btn-sm flex-1" disabled>현재 적용됨</button>'
              : `<button class="btn-primary btn-sm flex-1 select-char-btn" data-id="${key}">선택하여 사용</button>`
          }
          ${
            !isPreset
              ? `<button class="btn-danger btn-sm delete-char-btn" data-id="${key}" data-name="${charDef.name}">삭제</button>`
              : ""
          }
        </div>
      `;

      charGrid.appendChild(card);
    }

    // Attach listeners
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

    document.querySelectorAll(".delete-char-btn").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const id = btn.dataset.id;
        const name = btn.dataset.name;
        if (confirm(`정말로 "${name}" 캐릭터를 삭제하시겠습니까?`)) {
          try {
            await invoke("delete_custom_character", { id });
            showToast(`"${name}" 캐릭터가 삭제되었습니다.`);
            loadGallery();
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
    showToast("✨ Live2D 모델 및 모션 자동 감지 완료!");
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
// Real-time Event Listener & Lifecycle Sync
// ==========================================================================
listen("characters-updated", () => {
  loadGallery();
});

listen("switch-character", () => {
  loadGallery();
});

window.addEventListener("focus", () => {
  loadGallery();
});

document.addEventListener("visibilitychange", () => {
  if (!document.hidden) {
    loadGallery();
  }
});

// Initial load
loadGallery();
