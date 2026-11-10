const api = globalThis.browser ?? globalThis.chrome;
let fenbiTab = null;

async function waitForTab(tabId) {
  await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("粉笔页面加载超时")), 30000);
    const listener = (id, info) => {
      if (id !== tabId || info.status !== "complete") return;
      clearTimeout(timer);
      api.tabs.onUpdated.removeListener(listener);
      resolve();
    };
    api.tabs.onUpdated.addListener(listener);
  });
}

async function ensureFenbiTab() {
  if (fenbiTab !== null) return fenbiTab;
  const tabs = await api.tabs.query({ url: "https://www.fenbi.com/*" });
  let tab = tabs[0];
  if (!tab) {
    tab = await api.tabs.create({
      url: "https://www.fenbi.com/",
      active: false,
    });
  }
  if (tab.status !== "complete") await waitForTab(tab.id);
  fenbiTab = tab.id;
  return fenbiTab;
}

globalThis.__mcExtensionHost = {
  async fetch(host, path, query) {
    const tabId = await ensureFenbiTab();
    let last;
    for (let attempt = 0; attempt < 100; attempt += 1) {
      try {
        const response = await api.tabs.sendMessage(tabId, {
          type: "fenbi-fetch",
          host,
          path,
          query,
        });
        if (!response.ok) throw new Error(response.error);
        return { status: response.status, body: response.body };
      } catch (error) {
        last = error;
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
    }
    throw last;
  },
};
