(async () => {
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const heading = () => document.querySelector(".qb-page h1")?.textContent?.trim();
  const report = { ok: false, pages: [], platform: null, error: null };
  try {
    report.platform = await invoke("platform_capabilities");
    if (report.platform.os !== "macos" || report.platform.arch !== "aarch64") {
      throw new Error("The installed application is not native macOS arm64.");
    }
    await invoke("settings_load");
    await invoke("workspace_state");
    for (const [path, title] of [["/software", "软件与安装"], ["/settings", "设置"], ["/relays", "中转站"]]) {
      location.hash = path;
      let ready = false;
      for (let attempt = 0; attempt < 160; attempt++) {
        await wait(250);
        if (heading()?.includes(title)) { ready = true; break; }
      }
      if (!ready) throw new Error(`Native WebView did not render ${path}: ${document.body.innerText.slice(0, 1000)}`);
      report.pages.push({ path, heading: heading() });
    }
    report.ok = true;
  } catch (error) {
    report.error = String(error);
  }
  await invoke("plugin:event|emit", { event: "qb-macos-ui-smoke", payload: report });
})();
