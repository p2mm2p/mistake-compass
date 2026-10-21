// 画板：在 #qcard 上盖一层 canvas，用 atrament 统一处理 Pointer 输入。
// 笔迹只留在画布上，退出即销毁；事件委托到 document，htmx 换掉 #panel 后不用重新绑定。
import Atrament from "./atrament.esm.min.js";

(function () {
  let canvas = null;
  let ink = null;

  function mount() {
    const card = canvas.parentElement;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.max(1, Math.round(card.clientWidth * dpr));
    canvas.height = Math.max(1, Math.round(card.clientHeight * dpr));
    ink = new Atrament(canvas, {
      width: canvas.width,
      height: canvas.height,
      color: "#e5484d",
      weight: 2,
      adaptiveStroke: false,
      smoothing: 0.5,
      pressureLow: 1,
      pressureHigh: 1,
    });
  }

  // 重设画布尺寸会清空笔迹；销毁旧实例再挂新实例，避免监听器重复。
  function resize() {
    if (!canvas) return;
    if (ink) ink.destroy();
    ink = null;
    mount();
  }

  function enter() {
    const card = document.getElementById("qcard");
    if (!card || canvas) return;
    canvas = document.createElement("canvas");
    canvas.className = "board";
    // 先入 DOM 再量尺寸：mount 要从 canvas.parentElement 取卡片。
    card.appendChild(canvas);
    mount();
    card.classList.add("board-on");
  }

  function exit() {
    if (ink) ink.destroy();
    if (canvas) canvas.remove();
    ink = canvas = null;
    const card = document.getElementById("qcard");
    if (card) card.classList.remove("board-on");
  }

  document.addEventListener("click", function (event) {
    const button = event.target.closest("[data-board]");
    if (!button) return;
    if (button.dataset.board === "enter") enter();
    else if (button.dataset.board === "exit") exit();
    else if (ink) ink.clear();
  });

  window.addEventListener("resize", resize);

  // 换题替换 #panel 后，画布已不在文档里，模块状态同步清空。
  document.addEventListener("htmx:afterSwap", function () {
    if (canvas && !canvas.isConnected) exit();
  });
})();
