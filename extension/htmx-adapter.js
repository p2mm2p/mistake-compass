let activeSubmitter = null;

document.addEventListener(
  "click",
  (event) => {
    const submitter = event.target.closest(
      "button[type=submit], input[type=submit]",
    );
    if (submitter?.form) {
      submitter.form.__mcSubmitter = submitter;
      activeSubmitter = submitter;
    }
  },
  true,
);

export function installHtmxAdapter(handleRequest) {
  document.addEventListener("htmx:beforeRequest", async (event) => {
    const elt = event.detail.elt || event.target;
    const config = event.detail.requestConfig;
    event.preventDefault();

    const method = (config?.verb || event.detail.verb || "get").toUpperCase();
    const path = config?.path || event.detail.pathInfo?.requestPath;
    const form = elt instanceof HTMLFormElement ? elt : elt.closest("form");
    let body = "";
    if (method !== "GET" && form) {
      const params = new URLSearchParams();
      for (const [key, value] of new FormData(form)) {
        params.append(key, value);
      }
      const submitter = activeSubmitter || form.__mcSubmitter;
      if (submitter?.name) params.append(submitter.name, submitter.value);
      body = params.toString();
      activeSubmitter = null;
    }

    try {
      globalThis.__mcRequestCount = (globalThis.__mcRequestCount || 0) + 1;
      const result = JSON.parse(await handleRequest(method, path, body));
      globalThis.htmx.swap(event.detail.target, result.html, {
        swapStyle: elt.getAttribute("hx-swap") || "innerHTML",
      });
    } catch (error) {
      console.error(error);
    }
  });
}
