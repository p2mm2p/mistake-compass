const ALLOWED_HOSTS = new Set(["login.fenbi.com", "tiku.fenbi.com"]);

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type !== "fenbi-fetch") return;

  (async () => {
    const url = new URL(`https://${message.host}${message.path}`);
    if (!ALLOWED_HOSTS.has(url.hostname)) {
      sendResponse({ ok: false, error: "不允许的请求地址" });
      return;
    }
    for (const [key, value] of message.query ?? []) {
      url.searchParams.append(key, value);
    }
    try {
      const response = await fetch(url, {
        credentials: "include",
        headers: { Accept: "application/json, text/plain, */*" },
      });
      sendResponse({
        ok: true,
        status: response.status,
        body: await response.text(),
      });
    } catch (error) {
      sendResponse({ ok: false, error: String(error) });
    }
  })();

  return true;
});
