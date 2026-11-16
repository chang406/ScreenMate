/**
 * ScreenMate Live2D 런타임 로더 (오프라인 우선)
 *
 * 로드 순서 (pixi-live2d-display cubism4 플러그인은 Cubism Core가 먼저 있어야 함):
 *   1) Live2D Cubism Core : lib/live2dcubismcore.min.js (사용자가 직접 배치, 선택) → 없으면 공식 CDN
 *   2) Pixi.js 6.5.9      : lib/vendor/pixi.min.js (레포 포함, MIT) → 실패 시 CDN
 *   3) pixi-live2d-display 0.4.0 cubism4 : lib/vendor/… (레포 포함, MIT) → 실패 시 CDN
 *
 * 결과는 `window.__live2dReady` (Promise<{ ok, pixi, core, plugin, sources, error }>) 로 노출됩니다.
 * main.js는 이 결과를 보고 Live2D 캐릭터를 띄우거나, 불가능하면 2D 캐릭터로 우아하게 대체합니다.
 *
 * Cubism Core는 Live2D Inc.의 독점 라이선스 파일이라 레포에 포함하지 않습니다.
 * 완전 오프라인/폐쇄망에서 Live2D 캐릭터를 쓰려면 `src/lib/README.md`를 참고해 파일을 직접 배치하세요.
 */
(function () {
  "use strict";

  var SOURCES = {
    core: [
      "lib/live2dcubismcore.min.js",
      "https://cubism.live2d.com/sdk-web/cubismcore/live2dcubismcore.min.js",
    ],
    pixi: [
      "lib/vendor/pixi.min.js",
      "https://cdn.jsdelivr.net/npm/pixi.js@6.5.9/dist/browser/pixi.min.js",
    ],
    plugin: [
      "lib/vendor/pixi-live2d-display.cubism4.min.js",
      "https://cdn.jsdelivr.net/npm/pixi-live2d-display@0.4.0/dist/cubism4.min.js",
    ],
  };

  function loadScript(src, timeoutMs) {
    return new Promise(function (resolve, reject) {
      var el = document.createElement("script");
      var timer = setTimeout(function () {
        el.onload = el.onerror = null;
        reject(new Error("timeout: " + src));
      }, timeoutMs);
      el.src = src;
      el.async = false;
      el.onload = function () {
        clearTimeout(timer);
        resolve(src);
      };
      el.onerror = function () {
        clearTimeout(timer);
        el.remove();
        reject(new Error("failed: " + src));
      };
      document.head.appendChild(el);
    });
  }

  /** Tries each candidate in order until `isLoaded()` becomes true. */
  async function loadFirst(candidates, isLoaded) {
    for (var i = 0; i < candidates.length; i++) {
      if (isLoaded()) return candidates[i - 1] || "preloaded";
      try {
        // 로컬 파일은 즉시 응답, 원격 CDN은 네트워크가 막힌 환경에서 오래 매달리지 않도록 제한
        var isRemote = /^https?:/.test(candidates[i]);
        await loadScript(candidates[i], isRemote ? 6000 : 3000);
        if (isLoaded()) return candidates[i];
      } catch (e) {
        /* 다음 후보로 */
      }
    }
    return null;
  }

  window.__live2dReady = (async function () {
    var result = { ok: false, pixi: false, core: false, plugin: false, sources: {}, error: null };
    try {
      result.sources.core = await loadFirst(SOURCES.core, function () {
        return typeof window.Live2DCubismCore !== "undefined";
      });
      result.core = !!result.sources.core;

      result.sources.pixi = await loadFirst(SOURCES.pixi, function () {
        return typeof window.PIXI !== "undefined" && !!window.PIXI.Application;
      });
      result.pixi = !!result.sources.pixi;

      if (result.core && result.pixi) {
        result.sources.plugin = await loadFirst(SOURCES.plugin, function () {
          return !!(window.PIXI && window.PIXI.live2d && window.PIXI.live2d.Live2DModel);
        });
        result.plugin = !!result.sources.plugin;
      }
      result.ok = result.core && result.pixi && result.plugin;
      if (!result.ok) {
        result.error = !result.core
          ? "Live2D Cubism Core를 불러오지 못했습니다 (오프라인?)"
          : !result.pixi
          ? "Pixi.js를 불러오지 못했습니다"
          : "pixi-live2d-display를 불러오지 못했습니다";
      }
    } catch (e) {
      result.error = String((e && e.message) || e);
    }
    console.log("[live2d-loader]", JSON.stringify(result));
    return result;
  })();
})();
