import init, {
  export_backup,
  handle_request,
  import_backup,
  refresh_ui,
  render_initial,
  request_persistent_storage,
  run_sync,
} from "./pkg/mc_wasm.js";
import { installHtmxAdapter } from "./htmx-adapter.js";

globalThis.__mcProgress = (event) => {
  const progress = document.getElementById("sync-progress");
  const status = document.getElementById("sync-status");
  let value;
  try {
    value = JSON.parse(event);
  } catch {
    return;
  }
  if (value.kind === "error") {
    status.textContent = value.message;
  } else if (value.kind === "batch") {
    status.textContent = `已完成 ${value.completed} / ${value.total} 批`;
  } else if (value.kind === "phase") {
    status.textContent = value.value;
  }
  progress.textContent += `${event}\n`;
};

function renderPage(payload) {
  const data = JSON.parse(payload);
  const app = document.getElementById("app");
  if (!data.hasCorpus) {
    app.innerHTML =
      '<div class="empty-state"><h1>还没有题库</h1><p class="hint">先导入备份，或点击“抓取错题”。</p></div>';
    return;
  }
  const documentNode = new DOMParser().parseFromString(data.html, "text/html");
  app.innerHTML = documentNode.body.innerHTML;
  document.title = documentNode.title || "错题罗盘";
  if (globalThis.htmx) globalThis.htmx.process(app);
}

function bindControls() {
  const syncButton = document.getElementById("sync-button");
  const exportButton = document.getElementById("export-button");
  const importInput = document.getElementById("import-input");
  syncButton.disabled = false;
  exportButton.disabled = false;
  importInput.disabled = false;

  syncButton.addEventListener("click", async () => {
    const status = document.getElementById("sync-status");
    const progress = document.getElementById("sync-progress");
    syncButton.disabled = true;
    progress.textContent = "";
    status.textContent = "准备抓取";
    try {
      const report = await run_sync(`g${Date.now()}`);
      renderPage(await refresh_ui());
      status.textContent = `抓取完成：${JSON.parse(report).questions} 题`;
      document.documentElement.dataset.syncState = "done";
    } catch (error) {
      status.textContent = String(error);
      document.documentElement.dataset.syncState = "error";
    } finally {
      syncButton.disabled = false;
    }
  });

  exportButton.addEventListener("click", async () => {
    const status = document.getElementById("backup-status");
    try {
      const bytes = await export_backup();
      const blob = new Blob([bytes], {
        type: "application/zip",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = "mistake-compass-backup.zip";
      link.click();
      URL.revokeObjectURL(url);
      status.textContent = `已导出 ${bytes.byteLength} 字节`;
      document.documentElement.dataset.backupState = "exported";
    } catch (error) {
      status.textContent = String(error);
      document.documentElement.dataset.backupState = "error";
    }
  });

  importInput.addEventListener("change", async () => {
    const status = document.getElementById("backup-status");
    const file = importInput.files?.[0];
    if (!file) return;
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      const summary = await import_backup(bytes);
      renderPage(await refresh_ui());
      status.textContent = `导入完成：${JSON.parse(summary).questions} 题`;
      document.documentElement.dataset.backupState = "imported";
    } catch (error) {
      status.textContent = String(error);
      document.documentElement.dataset.backupState = "error";
    }
  });
}

init()
  .then(async () => {
    installHtmxAdapter(handle_request);
    bindControls();
    request_persistent_storage()
      .then((persisted) => {
        document.getElementById("persist-status").textContent = persisted
          ? "持久化存储已允许"
          : "持久化存储未授予";
        document.documentElement.dataset.persist = String(persisted);
      })
      .catch((error) => {
        document.getElementById("persist-status").textContent = String(error);
        document.documentElement.dataset.persist = "error";
      });
    renderPage(await render_initial());
    document.documentElement.dataset.mcReady = "ready";
  })
  .catch((error) => {
    document.documentElement.dataset.mcReady = "error";
    document.getElementById("app").textContent = `WASM 初始化失败：${error}`;
  });
