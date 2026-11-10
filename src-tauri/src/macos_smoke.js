(async () => {
  if (document.readyState === "loading" || !window.__TAURI_INTERNALS__?.invoke || window.__QB_MACOS_SMOKE_STARTED__) return;
  window.__QB_MACOS_SMOKE_STARTED__ = true;
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const report = { ok: false, pages: [], platform: null, error: null };
  try {
    report.platform = await invoke("platform_capabilities");
    if (report.platform.os !== "macos" || report.platform.arch !== "aarch64") {
      throw new Error("The installed application is not native macOS arm64.");
    }
    await invoke("settings_load");
    await invoke("workspace_state");
    for (const [path, selector, title] of [
      ["/software", ".qb-page h1", "软件与安装"],
      ["/settings", ".qb-page h1", "设置"],
      ["/relays", ".qb-page .qb-st-ctabs[role=tablist]", "Claude Code"],
    ]) {
      location.hash = path;
      const content = () => document.querySelector(selector)?.textContent?.trim();
      let ready = false;
      for (let attempt = 0; attempt < 160; attempt++) {
        await wait(250);
        if (content()?.includes(title)) { ready = true; break; }
      }
      if (!ready) throw new Error(`Native WebView did not render ${path}: ${document.body.innerText.slice(0, 1000)}`);
      report.pages.push({ path, selector, content: content() });
    }
    report.ok = true;
  } catch (error) {
    report.error = String(error);
  }
  await invoke("plugin:event|emit", { event: "qb-macos-ui-smoke", payload: report });
})();
