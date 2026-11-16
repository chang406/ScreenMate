/**
 * ScreenMate 픽셀 단위 클릭 판정 (Pixel-accurate hit test)
 *
 * 마우스가 "캐릭터가 실제로 그려진 픽셀" 위에 있을 때만 클릭/드래그를 받고, 나머지 투명한 곳은
 * 뒤의 작업 창으로 클릭을 통과시키기 위한 판정 모듈입니다.
 *
 * - 캐릭터별 설정이 필요 없습니다. 화면에 그려진 결과(알파값)만 보므로
 *   기본 캐릭터든, 캐릭터 매니저로 새로 추가한 2D 이미지/Live2D 캐릭터든 똑같이 동작합니다.
 * - 여유 범위(tolerance): 그려진 픽셀에서 몇 px 떨어진 곳까지는 캐릭터로 봅니다.
 *   팔·꼬리 바로 옆을 눌러도 잡히고, 흔들리는 꼬리나 머리카락도 놓치지 않게 하기 위함입니다.
 * - 판정할 수 없는 상황(이미지 로드 전, 그래픽 드라이버가 픽셀 읽기를 막는 경우 등)에서는
 *   호출 측이 기존 사각형 판정으로 되돌아가도록 `null`을 반환합니다.
 *
 * 사용: window.ScreenMateHitTest.image(imgEl, x, y) / .live2d(app, model, x, y)
 *       (x, y는 창 기준 CSS 픽셀, 반환값 true | false | null)
 */
(function () {
  "use strict";

  /** 그려진 픽셀로부터 이 거리(CSS px) 안이면 캐릭터 위로 판정 */
  var TOLERANCE_PX = 6;
  /** 이 값보다 불투명해야 "그려진 픽셀"로 인정 (0~255). 그림자·반투명 가장자리는 제외 */
  var ALPHA_THRESHOLD = 40;
  /** 2D 이미지 알파 마스크의 최대 변 길이. 메모리/속도와 정밀도의 절충 */
  var MASK_MAX_SIDE = 384;

  // ---------------------------------------------------------------------------
  // 2D 이미지: 이미지마다 한 번 알파 마스크를 만들어 캐시
  // ---------------------------------------------------------------------------

  /** src -> { w, h, alpha: Uint8Array } | "pending" | "failed" */
  var maskCache = new Map();

  function buildMask(imgEl) {
    var src = imgEl.currentSrc || imgEl.src;
    var nw = imgEl.naturalWidth;
    var nh = imgEl.naturalHeight;
    if (!src || !imgEl.complete || !nw || !nh) return null;
    var k = Math.min(1, MASK_MAX_SIDE / Math.max(nw, nh));
    var w = Math.max(1, Math.round(nw * k));
    var h = Math.max(1, Math.round(nh * k));
    try {
      var c = document.createElement("canvas");
      c.width = w;
      c.height = h;
      var ctx = c.getContext("2d", { willReadFrequently: true });
      ctx.drawImage(imgEl, 0, 0, w, h);
      var data = ctx.getImageData(0, 0, w, h).data;
      var alpha = new Uint8Array(w * h);
      for (var i = 0; i < alpha.length; i++) alpha[i] = data[i * 4 + 3];
      var mask = { w: w, h: h, alpha: alpha };
      maskCache.set(src, mask);
      return mask;
    } catch (e) {
      // 외부 출처 이미지 등으로 픽셀을 읽을 수 없는 경우
      maskCache.set(src, "failed");
      return null;
    }
  }

  function getMask(imgEl) {
    var src = imgEl.currentSrc || imgEl.src;
    var cached = maskCache.get(src);
    if (cached === "failed") return null;
    if (cached) return cached;
    return buildMask(imgEl);
  }

  /**
   * @returns {boolean|null} 픽셀 판정 결과, 판정 불가면 null
   */
  function imageHit(imgEl, x, y) {
    if (!imgEl || imgEl.classList.contains("hidden")) return null;
    var rect = imgEl.getBoundingClientRect();
    if (rect.width <= 0 || rect.height <= 0) return null;
    // 사각형 + 여유 범위 밖이면 계산할 필요 없음
    if (
      x < rect.left - TOLERANCE_PX ||
      x > rect.right + TOLERANCE_PX ||
      y < rect.top - TOLERANCE_PX ||
      y > rect.bottom + TOLERANCE_PX
    ) {
      return false;
    }
    var mask = getMask(imgEl);
    if (!mask) return null;

    // 화면 좌표 → 마스크 좌표 (CSS transform scale/translate 반영: getBoundingClientRect 기준)
    var sx = mask.w / rect.width;
    var sy = mask.h / rect.height;
    var mx = Math.floor((x - rect.left) * sx);
    var my = Math.floor((y - rect.top) * sy);
    var rx = Math.max(1, Math.ceil(TOLERANCE_PX * sx));
    var ry = Math.max(1, Math.ceil(TOLERANCE_PX * sy));

    var x0 = Math.max(0, mx - rx);
    var x1 = Math.min(mask.w - 1, mx + rx);
    var y0 = Math.max(0, my - ry);
    var y1 = Math.min(mask.h - 1, my + ry);
    for (var py = y0; py <= y1; py++) {
      var row = py * mask.w;
      for (var px = x0; px <= x1; px++) {
        if (mask.alpha[row + px] > ALPHA_THRESHOLD) return true;
      }
    }
    return false;
  }

  // ---------------------------------------------------------------------------
  // Live2D: 지금 화면에 그려진 WebGL 픽셀을 마우스 주변만 조금 읽어서 판정
  // (매 프레임 모양이 바뀌는 모션/물리 효과를 그대로 반영)
  // ---------------------------------------------------------------------------

  var lastLive2D = { x: NaN, y: NaN, t: 0, result: null };
  var readBuf = null;

  /**
   * @param app  PIXI.Application (preserveDrawingBuffer: true 로 생성되어야 함)
   * @param model Live2D 모델 (스테이지 좌표 = CSS 픽셀)
   * @returns {boolean|null}
   */
  function live2dHit(app, model, x, y) {
    if (!app || !model || !app.renderer || !app.renderer.gl) return null;

    // 1차: 모델 경계 상자 + 여유 범위 밖이면 즉시 false
    try {
      var b = model.getBounds();
      if (
        x < b.x - TOLERANCE_PX ||
        x > b.x + b.width + TOLERANCE_PX ||
        y < b.y - TOLERANCE_PX ||
        y > b.y + b.height + TOLERANCE_PX
      ) {
        return false;
      }
    } catch (e) {
      /* 경계 계산 실패 시 픽셀 판정으로 진행 */
    }

    // 같은 위치를 아주 짧은 간격으로 다시 묻는 경우 GPU 읽기를 아끼기 위해 재사용
    var now = performance.now();
    if (x === lastLive2D.x && y === lastLive2D.y && now - lastLive2D.t < 40) {
      return lastLive2D.result;
    }

    var gl = app.renderer.gl;
    var res = app.renderer.resolution || 1;
    var bufW = gl.drawingBufferWidth;
    var bufH = gl.drawingBufferHeight;
    var r = Math.max(1, Math.round(TOLERANCE_PX * res));
    var cx = Math.round(x * res);
    var cy = Math.round(bufH - y * res); // WebGL은 아래쪽이 원점
    var x0 = Math.max(0, cx - r);
    var y0 = Math.max(0, cy - r);
    var w = Math.min(bufW, cx + r + 1) - x0;
    var h = Math.min(bufH, cy + r + 1) - y0;
    if (w <= 0 || h <= 0) return false;

    var result = null;
    try {
      var need = w * h * 4;
      if (!readBuf || readBuf.length < need) readBuf = new Uint8Array(need);
      gl.readPixels(x0, y0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, readBuf);
      result = false;
      for (var i = 3; i < need; i += 4) {
        if (readBuf[i] > ALPHA_THRESHOLD) {
          result = true;
          break;
        }
      }
    } catch (e) {
      result = null;
    }
    lastLive2D = { x: x, y: y, t: now, result: result };
    return result;
  }

  window.ScreenMateHitTest = {
    TOLERANCE_PX: TOLERANCE_PX,
    image: imageHit,
    live2d: live2dHit,
    /** 캐릭터 이미지 교체/삭제 시 오래된 마스크 정리 */
    clearCache: function () {
      maskCache.clear();
    },
  };
})();
