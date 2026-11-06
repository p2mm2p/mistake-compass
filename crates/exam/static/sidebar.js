// 侧边栏收起状态放在 .shell 上，htmx 替换 #drawer 后仍然保留。
(function () {
  const toggleSelector = "[data-drawer-toggle]";

  function syncButtons() {
    document.querySelectorAll(toggleSelector).forEach(function (button) {
      const shell = button.closest(".shell");
      if (!shell) return;
      const collapsed = shell.classList.contains("drawer-collapsed");
      const label = collapsed ? "展开侧边栏" : "收起侧边栏";
      button.setAttribute("aria-expanded", String(!collapsed));
      button.setAttribute("aria-label", label);
      button.title = label;
    });
  }

  document.addEventListener("click", function (event) {
    const button = event.target.closest(toggleSelector);
    if (!button) return;
    const shell = button.closest(".shell");
    if (!shell) return;

    const collapsed = shell.classList.toggle("drawer-collapsed");
    syncButtons();
    document.dispatchEvent(
      new CustomEvent("drawer:toggled", { detail: { collapsed: collapsed } })
    );
  });

  document.addEventListener("htmx:afterSwap", syncButtons);
  syncButtons();
})();
