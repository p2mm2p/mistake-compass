// 画板：在 #qcard 上盖一层 canvas，按住鼠标左键图画。
// 笔迹只留在画布上，退出即销毁；事件委托到 document，htmx 换掉 #panel 后不用重新绑定。
(function () {
  let canvas = null;
  let ctx = null;
  let drawing = false;
  let last = { x: 0, y: 0 };

  function point(event) {
    const box = canvas.getBoundingClientRect();
    return { x: event.clientX - box.left, y: event.clientY - box.top };
  }

  // 重设 canvas 尺寸会重置 2d 上下文的样式，所以画完尺寸后要重新设笔。
  function brush() {
    ctx.lineWidth = 2;
    ctx.lineCap = "round";
    ctx.strokeStyle = "#e5484d";
  }

  function resize() {
    const card = canvas.parentElement;
    canvas.width = card.clientWidth;
    canvas.height = card.clientHeight;
    brush();
  }

  function enter() {
    const card = document.getElementById("qcard");
    if (!card || canvas) return;
    canvas = document.createElement("canvas");
    canvas.className = "board";
    ctx = canvas.getContext("2d");
    // 先入 DOM 再量尺寸：resize 要从 canvas.parentElement 取卡片。
    card.appendChild(canvas);
    resize();
    card.classList.add("board-on");
  }

  function exit() {
    if (canvas) canvas.remove();
    canvas = ctx = null;
    drawing = false;
    const card = document.getElementById("qcard");
    if (card) card.classList.remove("board-on");
  }

  function clear() {
    if (ctx) ctx.clearRect(0, 0, canvas.width, canvas.height);
  }

  document.addEventListener("click", function (event) {
    const button = event.target.closest("[data-board]");
    if (!button) return;
    if (button.dataset.board === "enter") enter();
    else if (button.dataset.board === "exit") exit();
    else clear();
  });

  document.addEventListener("mousedown", function (event) {
    if (!canvas || event.button !== 0 || event.target !== canvas) return;
    drawing = true;
    last = point(event);
    event.preventDefault();
  });

  document.addEventListener("mousemove", function (event) {
    if (!drawing) return;
    const now = point(event);
    ctx.beginPath();
    ctx.moveTo(last.x, last.y);
    ctx.lineTo(now.x, now.y);
    ctx.stroke();
    last = now;
  });

  document.addEventListener("mouseup", function () {
    drawing = false;
  });

  // 窗口变化时重取画布尺寸；重设尺寸会清空笔迹，可避免笔迹与光标错位。
  window.addEventListener("resize", function () {
    if (canvas) resize();
  });

  // 换题替换 #panel 后，画布已不在文档里，模块状态同步清空。
  document.addEventListener("htmx:afterSwap", function () {
    if (canvas && !canvas.isConnected) exit();
  });
})();
