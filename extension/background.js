function openApp() {
  chrome.tabs.create({ url: chrome.runtime.getURL("app.html") });
}

chrome.action.onClicked.addListener(openApp);
if (globalThis.browser) {
  chrome.runtime.onInstalled.addListener(() => setTimeout(openApp, 2000));
} else {
  chrome.runtime.onInstalled.addListener(openApp);
}
