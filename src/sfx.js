/**
 * ScreenMate 효과음 (SFX) — WebAudio로 즉석 합성하는 짧은 알림음
 *
 * - 에셋 파일 없음(초경량), 네트워크 불필요.
 * - 기본값 OFF. 우클릭 메뉴에서 켜고 볼륨 조절 (localStorage 저장).
 * - 긴 음성(TTS) 대신 0.5초 이하의 짧은 소리만 사용 (ROADMAP: "짧고 경쾌한 효과음").
 *
 * 사용: window.ScreenMateSfx.play("complete" | "error" | "click")
 */
(function () {
  "use strict";

  var STORAGE_ENABLED = "screenmate_sfx_enabled";
  var STORAGE_VOLUME = "screenmate_sfx_volume";
  var MIN_GAP_MS = 700; // 같은 소리가 연달아 울리지 않도록

  function readStorage(key, fallback) {
    try {
      var v = localStorage.getItem(key);
      return v === null ? fallback : v;
    } catch (e) {
      return fallback;
    }
  }
  function writeStorage(key, value) {
    try {
      localStorage.setItem(key, String(value));
    } catch (e) {
      /* 저장 불가 환경 무시 */
    }
  }

  var enabled = readStorage(STORAGE_ENABLED, "false") === "true";
  var volume = Math.min(1, Math.max(0, parseFloat(readStorage(STORAGE_VOLUME, "0.5")) || 0));
  var ctx = null;
  var lastPlayed = {};

  function getContext() {
    var AC = window.AudioContext || window.webkitAudioContext;
    if (!AC) return null;
    if (!ctx) ctx = new AC();
    if (ctx.state === "suspended") ctx.resume().catch(function () {});
    return ctx;
  }

  /** 단일 음: 빠른 어택 + 지수 감쇠 엔벨로프 */
  function tone(ac, freq, start, duration, type, gainScale) {
    var osc = ac.createOscillator();
    var gain = ac.createGain();
    osc.type = type || "sine";
    osc.frequency.setValueAtTime(freq, start);
    var peak = Math.max(0.0002, volume * 0.35 * (gainScale || 1));
    gain.gain.setValueAtTime(0.0001, start);
    gain.gain.exponentialRampToValueAtTime(peak, start + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.0001, start + duration);
    osc.connect(gain).connect(ac.destination);
    osc.start(start);
    osc.stop(start + duration + 0.02);
  }

  var SOUNDS = {
    // 작업 완료: 맑게 올라가는 3화음 아르페지오 (C6 → E6 → G6)
    complete: function (ac, t) {
      tone(ac, 1046.5, t, 0.28, "sine");
      tone(ac, 1318.5, t + 0.08, 0.3, "sine");
      tone(ac, 1568.0, t + 0.16, 0.42, "sine", 0.9);
    },
    // 에러: 낮게 내려가는 두 음 (당황)
    error: function (ac, t) {
      tone(ac, 392.0, t, 0.18, "triangle", 1.1);
      tone(ac, 311.1, t + 0.15, 0.3, "triangle", 1.1);
    },
    // 클릭: 짧고 통통 튀는 팝
    click: function (ac, t) {
      var osc = ac.createOscillator();
      var gain = ac.createGain();
      osc.type = "sine";
      osc.frequency.setValueAtTime(660, t);
      osc.frequency.exponentialRampToValueAtTime(1320, t + 0.07);
      gain.gain.setValueAtTime(0.0001, t);
      gain.gain.exponentialRampToValueAtTime(Math.max(0.0002, volume * 0.3), t + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.0001, t + 0.12);
      osc.connect(gain).connect(ac.destination);
      osc.start(t);
      osc.stop(t + 0.14);
    },
  };

  function play(name, opts) {
    var force = opts && opts.force;
    if ((!enabled && !force) || volume <= 0 || !SOUNDS[name]) return false;
    var now = Date.now();
    if (!force) {
      if (lastPlayed[name] && now - lastPlayed[name] < MIN_GAP_MS) return false;
      lastPlayed[name] = now; // 미리 듣기(force)는 실제 알림을 막지 않도록 기록하지 않음
    }
    try {
      var ac = getContext();
      if (!ac) return false;
      SOUNDS[name](ac, ac.currentTime + 0.01);
      return true;
    } catch (e) {
      console.warn("[sfx] play failed:", e);
      return false;
    }
  }

  // 일부 WebView는 사용자 입력 전 오디오를 막으므로 첫 클릭 때 컨텍스트를 깨워 둔다.
  document.addEventListener(
    "mousedown",
    function () {
      if (enabled) getContext();
    },
    { capture: true }
  );

  window.ScreenMateSfx = {
    play: play,
    isEnabled: function () {
      return enabled;
    },
    setEnabled: function (on) {
      enabled = !!on;
      writeStorage(STORAGE_ENABLED, enabled);
      if (enabled) getContext();
    },
    getVolume: function () {
      return volume;
    },
    setVolume: function (v) {
      volume = Math.min(1, Math.max(0, Number(v) || 0));
      writeStorage(STORAGE_VOLUME, volume);
    },
    /** 상태 전환 → 효과음 매핑 */
    forState: function (state) {
      if (state === "talking") return play("complete");
      if (state === "error") return play("error");
      if (state === "click_react") return play("click");
      return false;
    },
  };
})();
