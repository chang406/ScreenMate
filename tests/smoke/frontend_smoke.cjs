#!/usr/bin/env node
/**
 * ScreenMate 프론트엔드 오프라인 스모크 테스트 (헤드리스 Chromium + Playwright)
 *
 * - `src/`를 로컬 정적 서버로 띄우고, 외부 네트워크(CDN 등) 요청을 전부 차단해 "오프라인" 환경을 재현합니다.
 * - `window.__TAURI__`를 목(mock)으로 주입하여 Rust 백엔드 없이 오버레이(index.html)를 구동합니다.
 * - 캐릭터별로 상태 이벤트(thinking → busy → talking → error)를 흘려보내고
 *   말풍선 텍스트, 캐릭터 표시 여부, 런타임 에러, 차단된 외부 요청을 수집합니다.
 *
 * 사용법 (프로젝트 루트에서):
 *   node tests/smoke/frontend_smoke.cjs --label before --out docs/autonomous_run/screens
 *
 * 출력: 콘솔에 JSON 요약, --out 디렉토리에 `<label>_<character>.png` 스크린샷.
 * 필요: `playwright` 패키지와 Chromium (npm i -D playwright && npx playwright install chromium)
 */
const http = require("http");
const fs = require("fs");
const path = require("path");
const { chromium } = require("playwright");

const ROOT = path.resolve(__dirname, "..", "..");
const SRC = path.join(ROOT, "src");

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i > -1 && process.argv[i + 1] ? process.argv[i + 1] : fallback;
}
const LABEL = arg("label", "run");
const OUT = path.resolve(ROOT, arg("out", "docs/autonomous_run/screens"));
const CHARACTERS = arg("characters", "pixel_cat,mao").split(",");
// 창(뷰포트) 크기. 작은 노트북에 맞춰 줄어든 창을 재현할 때 사용 (예: --viewport 640x560)
const [VIEW_W, VIEW_H] = arg("viewport", "640x920").split("x").map(Number);

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".moc3": "application/octet-stream",
};

function startServer() {
  const server = http.createServer((req, res) => {
    const urlPath = decodeURIComponent(new URL(req.url, "http://x").pathname);
    const filePath = path.join(SRC, urlPath === "/" ? "index.html" : urlPath);
    if (!filePath.startsWith(SRC) || !fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
      res.writeHead(404);
      res.end("not found");
      return;
    }
    res.writeHead(200, { "Content-Type": MIME[path.extname(filePath)] || "application/octet-stream" });
    fs.createReadStream(filePath).pipe(res);
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

/** Rust `get_character_configs`와 동일한 방식으로 캐릭터 폴더를 스캔해 목 응답을 만든다. */
function buildCharacterConfigs(active) {
  const characters = {};
  const base = path.join(SRC, "characters");
  for (const name of fs.readdirSync(base)) {
    if (name === "custom_characters") continue;
    const p = path.join(base, name, "character.json");
    if (fs.existsSync(p)) characters[name] = JSON.parse(fs.readFileSync(p, "utf8"));
  }
  return { activeCharacter: active, characters, globalBubbleSettings: null };
}

function mockTauriScript(configs) {
  return `
    (() => {
      const listeners = {};
      const configs = ${JSON.stringify(configs)};
      window.__smoke = { invokes: [], bubbles: [], dragCalls: 0 };
      window.__TAURI__ = {
        core: {
          invoke: async (cmd, args) => {
            window.__smoke.invokes.push(cmd);
            if (cmd === "get_character_configs") return JSON.parse(JSON.stringify(configs));
            if (cmd === "get_app_settings") return {};
            return null;
          },
        },
        event: {
          listen: async (name, cb) => {
            (listeners[name] = listeners[name] || []).push(cb);
            return () => {};
          },
        },
        window: {
          getCurrentWindow: () => ({
            startDragging: async () => { window.__smoke.dragCalls++; },
            setPosition: async () => {},
          }),
        },
      };
      window.__mockEmit = (name, payload) => (listeners[name] || []).forEach((cb) => cb({ payload }));
      document.addEventListener("DOMContentLoaded", () => {
        const t = document.getElementById("bubble-text");
        if (!t) return;
        new MutationObserver(() => window.__smoke.bubbles.push(t.textContent)).observe(t, {
          childList: true, characterData: true, subtree: true,
        });
      });
    })();
  `;
}

async function runCharacter(browser, baseUrl, key) {
  const context = await browser.newContext({ viewport: { width: VIEW_W, height: VIEW_H }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  const blocked = [];
  const pageErrors = [];
  const consoleErrors = [];

  // 오프라인 재현: 로컬 서버 외 모든 요청 차단
  await page.route("**/*", (route) => {
    const u = route.request().url();
    if (u.startsWith(baseUrl)) return route.continue();
    blocked.push(u);
    return route.abort("internetdisconnected");
  });
  page.on("pageerror", (e) => pageErrors.push(String(e.message || e)));
  page.on("console", (m) => {
    if (m.type() === "error") consoleErrors.push(m.text());
  });

  await page.addInitScript(mockTauriScript(buildCharacterConfigs(key)));
  await page.goto(baseUrl + "/index.html");
  await page.waitForTimeout(3000);

  const steps = [
    ["thinking", null],
    ["busy", "edit"],
    ["talking", null],
    ["error", null],
  ];
  const stateLog = [];
  for (const [state, detail] of steps) {
    await page.evaluate(([s, d]) => window.__mockEmit("state-change", { state: s, detail: d }), [state, detail]);
    await page.waitForTimeout(1700); // thinking 최소 유지 1.5초 보장 이후 관찰
    stateLog.push({
      sent: state,
      bubble: await page.evaluate(() => document.getElementById("bubble-text")?.textContent || ""),
    });
  }

  const info = await page.evaluate(() => {
    const img = document.getElementById("image-character");
    const cvs = document.getElementById("live2d-canvas");
    const vis = (el) => !!el && !el.classList.contains("hidden") && el.getBoundingClientRect().height > 0;
    return {
      PIXI: typeof window.PIXI,
      PIXI_live2d: typeof (window.PIXI && window.PIXI.live2d),
      Live2DCubismCore: typeof window.Live2DCubismCore,
      imageVisible: vis(img) && img.naturalWidth > 0,
      canvasVisible: vis(cvs),
      charName: document.getElementById("char-name")?.textContent,
      bubbleHistory: window.__smoke.bubbles.filter((b, i, a) => b && a.indexOf(b) === i),
    };
  });

  fs.mkdirSync(OUT, { recursive: true });
  const shot = path.join(OUT, `${LABEL}_${key}.png`);
  await page.addStyleTag({ content: "html,body{background:#3b3f4a !important}" });
  await page.screenshot({ path: shot });

  const interactions = await checkInteractions(page, key);
  await context.close();

  return {
    character: key,
    ...info,
    interactions,
    stateLog,
    pageErrors,
    consoleErrors,
    blockedExternalRequests: blocked,
    screenshot: path.relative(ROOT, shot),
  };
}

/** Context-menu controls added after the baseline. Missing controls are reported as null. */
async function checkInteractions(page, key) {
  const has = (sel) => page.evaluate((s) => !!document.querySelector(s), sel);
  const center = await page.evaluate(() => {
    const el = [document.getElementById("image-character"), document.getElementById("live2d-canvas")].find(
      (e) => e && !e.classList.contains("hidden")
    );
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height * 0.75 };
  });
  const out = {};

  // 1) 우클릭 메뉴 열기 + 스크린샷
  await page.mouse.click(center.x, center.y, { button: "right" });
  await page.waitForTimeout(250);
  out.contextMenuOpens = await page.evaluate(() => !document.getElementById("context-menu").classList.contains("hidden"));
  if (key === CHARACTERS[0]) await page.screenshot({ path: path.join(OUT, `${LABEL}_context_menu.png`) });

  // 2) 크기 슬라이더
  if (await has("#scale-slider")) {
    out.scaleSliderTo150 = await page.evaluate(() => {
      const s = document.getElementById("scale-slider");
      s.value = "150";
      s.dispatchEvent(new Event("input", { bubbles: true }));
      return { stored: localStorage.getItem("screenmate_scale"), label: document.getElementById("scale-value").textContent };
    });
  } else out.scaleSliderTo150 = null;

  // 2D 캐릭터를 200%로 키워도 창(뷰포트) 안에 들어오는지
  out.imageFitsWindowAt200 = await page.evaluate(async () => {
    const img = document.getElementById("image-character");
    const s = document.getElementById("scale-slider");
    if (!img || img.classList.contains("hidden") || !s) return null;
    s.value = "200";
    s.dispatchEvent(new Event("input", { bubbles: true }));
    await new Promise((r) => setTimeout(r, 300));
    const r = img.getBoundingClientRect();
    return { top: Math.round(r.top), bottom: Math.round(r.bottom), viewportH: window.innerHeight, fits: r.top >= 0 && r.bottom <= window.innerHeight };
  });

  // 3) 위치 잠금 → 드래그 시도 시 startDragging 호출 안 됨
  const drag = async () => {
    await page.evaluate(() => document.getElementById("context-menu").classList.add("hidden"));
    const before = await page.evaluate(() => window.__smoke.dragCalls);
    await page.mouse.move(center.x, center.y);
    await page.mouse.down();
    await page.mouse.move(center.x + 30, center.y + 30, { steps: 4 });
    await page.mouse.up();
    return (await page.evaluate(() => window.__smoke.dragCalls)) - before;
  };
  out.dragCallsUnlocked = await drag();
  if (await has("#toggle-lock-btn")) {
    await page.evaluate(() => document.getElementById("toggle-lock-btn").click());
    out.lockedStored = await page.evaluate(() => localStorage.getItem("screenmate_position_locked"));
    out.dragCallsLocked = await drag();
    // 트레이 메뉴에서 잠금 해제 이벤트
    await page.evaluate(() => window.__mockEmit("tray-action", "toggle_position_lock"));
    out.unlockedViaTrayStored = await page.evaluate(() => localStorage.getItem("screenmate_position_locked"));
  } else {
    out.lockedStored = out.dragCallsLocked = out.unlockedViaTrayStored = null;
  }

  // 4) 효과음: 켜기 → talking 전환 시 실제 재생 호출
  if (await page.evaluate(() => !!window.ScreenMateSfx)) {
    out.sfx = await page.evaluate(async () => {
      const sfx = window.ScreenMateSfx;
      const played = [];
      const orig = sfx.forState;
      sfx.forState = (st) => {
        const r = orig(st);
        played.push([st, r]);
        return r;
      };
      document.getElementById("toggle-sfx-btn").click();
      const enabled = sfx.isEnabled();
      window.__mockEmit("state-change", { state: "talking", detail: null });
      await new Promise((r) => setTimeout(r, 300));
      return { enabled, played, audioContextAvailable: !!(window.AudioContext || window.webkitAudioContext) };
    });
  } else out.sfx = null;

  out.invokedCommands = await page.evaluate(() => [...new Set(window.__smoke.invokes)]);
  return out;
}

(async () => {
  const server = await startServer();
  const baseUrl = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch();
  const results = [];
  try {
    for (const key of CHARACTERS) results.push(await runCharacter(browser, baseUrl, key));
  } finally {
    await browser.close();
    server.close();
  }
  console.log(JSON.stringify({ label: LABEL, offline: true, viewport: `${VIEW_W}x${VIEW_H}`, results }, null, 2));
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
