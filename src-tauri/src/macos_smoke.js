(async () => {
  if (document.readyState === "loading" || !window.__TAURI_INTERNALS__?.invoke || window.__QB_MACOS_SMOKE_STARTED__) return;
  window.__QB_MACOS_SMOKE_STARTED__ = true;
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const config = window.__QB_MACOS_SMOKE_CONFIG__ || { phase: "smoke" };
  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const check = (value, message) => { if (!value) throw new Error(message); };
  const until = async (read, predicate, message) => {
    for (let attempt = 0; attempt < 160; attempt++) {
      const value = await read();
      if (predicate(value)) return value;
      await wait(250);
    }
    throw new Error(message);
  };
  const reject = async (command, args) => {
    let rejected = false;
    try { await invoke(command, args); } catch { rejected = true; }
    check(rejected, `${command} accepted an invalid operation`);
  };
  const report = { ok: false, phase: config.phase, pages: [], checks: [], platform: null, error: null };
  const pass = (name) => report.checks.push(name);
  try {
    report.platform = await invoke("platform_capabilities");
    check(report.platform.os === "macos" && report.platform.arch === "aarch64", "The installed application is not native macOS arm64.");
    await until(() => invoke("startup_status"), (status) => status.ready, "Native startup recovery did not complete");
    check(config.diagnostics?.endpoint && config.diagnostics?.secret, "Isolated supplementary-diagnostic fixtures were not initialized");
    const locale = await invoke("environment_client_locale", { expected: "ja-JP" });
    check(locale.items.length === 1 && locale.items[0].state === "pass" && locale.items[0].detail.includes("ja-jp"), "Native Mac locale IPC did not read the isolated Application Support/Claude/config.json");
    check(!JSON.stringify(locale).includes("native-locale-fixture-only"), "Locale IPC exposed an unrelated configuration field");
    const runtime = await invoke("environment_proxy_runtime", config.diagnostics);
    check(runtime.items.length >= 16 && runtime.items.every((item) => item.state === "pass"), "Native local-controller IPC did not verify the fixture's effective rules and group UDP support");
    check(!JSON.stringify(runtime).includes(config.diagnostics.secret), "Runtime-rule IPC exposed its transient controller secret");
    let asnError = "";
    try { await invoke("environment_asn", { ip: "127.0.0.1" }); } catch (error) { asnError = String(error?.message || error); }
    check(asnError.includes("公网 IP"), "ASN IPC failed to reject a local target before making a network request");
    pass("Native supplementary IPC: isolated Mac locale, authenticated loopback controller, rejected local ASN target; no public STUN/ASN requests");
    await invoke("settings_load");
    await invoke("workspace_state");
    for (const [path, selector, title] of [
      ["/", ".account-workspace", ""],
      ["/software", ".qb-page h1", "软件与安装"],
      ["/extensions", ".qb-page h1", "扩展中心"],
      ["/settings", ".qb-page h1", "设置"],
      ["/relays", ".qb-page .qb-st-ctabs[role=tablist]", "Claude Code"],
      ["/subscription", ".qb-page", "订阅"],
    ]) {
      location.hash = path;
      const content = () => document.querySelector(selector)?.textContent?.trim();
      await until(content, (value) => value !== undefined && value.includes(title), `Native WebView did not render ${path}`);
      report.pages.push({ path, selector, content: content().slice(0, 140) });
    }
    // The subscription acknowledgement is a local UI choice, unrelated to accounts.
    for (const dialog of document.querySelectorAll("dialog[open]")) {
      const button = [...dialog.querySelectorAll("button")].find((b) => /我读完了，进入指南/.test(b.textContent));
      button?.click();
    }
    location.hash = "/relays";
    await wait(300);
    if (config.phase === "create") {
      const target = "claude-code:default";
      const mcp = { id: "qb-native-mcp", name: "QB native MCP", server: { command: "/usr/bin/true", args: [] }, targets: [target], description: "Disposable fixture", homepage: null, tags: [] };
      let view = await invoke("mcp_save", { server: mcp, previousId: null });
      check(view.servers.some((s) => s.id === mcp.id && s.targets.includes(target)), "MCP save did not persist target");
      view = await invoke("mcp_save", { server: { ...mcp, name: "QB native MCP edited" }, previousId: mcp.id });
      check(view.servers.some((s) => s.id === mcp.id && s.name.endsWith("edited")), "MCP edit did not persist");
      await invoke("mcp_set_target", { id: mcp.id, target, enabled: false });
      await invoke("mcp_set_target", { id: mcp.id, target, enabled: true });
      pass("MCP encrypted save/edit/target removal and reapply through native IPC");

      const candidates = await invoke("skills_discover", { source: `${config.root}/skill-source` });
      check(candidates.some((s) => s.name === "qb-native-skill"), "Local Skill discovery failed");
      let skills = await invoke("skills_install", { picked: candidates, targets: [target] });
      check(skills.skills.some((s) => s.id === "qb-native-skill"), "Skill installation failed");
      skills = await invoke("skills_uninstall", { id: "qb-native-skill" });
      const backup = skills.backups.find((b) => b.skill === "qb-native-skill");
      check(backup, "Skill uninstall did not retain restore backup");
      await invoke("skills_restore", { backup: backup.id });
      await invoke("skills_set_target", { id: "qb-native-skill", target, enabled: true });
      pass("Local Skill discovery/install/uninstall/backup restore and target copy");

      let prompts = await invoke("prompts_save", { prompt: { id: "", name: "QB native prompt", content: "Native prompt v1", description: null, created_ms: 0, updated_ms: 0 } });
      let prompt = prompts.prompts.find((p) => p.name === "QB native prompt");
      prompts = await invoke("prompts_enable", { target, id: prompt.id });
      check(prompts.targets.some((t) => t.id === target && t.active === prompt.id), "Prompt activation failed");
      await reject("prompts_delete", { id: prompt.id });
      await invoke("prompts_save", { prompt: { ...prompt, content: "Native prompt v2" } });
      pass("Prompt create/activate/edit; active deletion rejected; original backed up");

      let file = await invoke("tool_files_save", { tool: "pi", name: "prompts/qb-native.md", content: "Native tool v1", revision: "missing" });
      await reject("tool_files_save", { tool: "pi", name: file.name, content: "stale write", revision: "missing" });
      file = await invoke("tool_files_save", { tool: "pi", name: file.name, content: "Native tool v2", revision: file.revision });
      pass("Pi tool file create/edit and stale revision refusal");

      const imported = await invoke("deeplink_paste", { url: "qbgate://v1/import?resource=provider&app=claude&name=QB-native-route&endpoint=https%3A%2F%2Fexample.invalid&apiKey=fixture-only-native-key" });
      check(imported.view.kind === "provider", "Fixture provider preview rejected");
      check(!JSON.stringify(imported).includes("fixture-only-native-key"), "Preview exposed unmasked fixture key");
      const outcome = await invoke("deeplink_import", { id: imported.id, choice: { group: "native-fixture", key: null } });
      check(outcome.route_id, "Explicitly confirmed provider import did not create route");
      let routes = await invoke("station_routes");
      let route = routes.find((r) => r.id === outcome.route_id);
      check(route && route.credential_id, "Imported route is missing credential");
      await invoke("station_put_route", { route: { ...route, nominal_rate: 0.75 }, previousId: route.id });
      pass("Relay masked preview/explicit import/encrypted credential/route edit (no upstream requests)");

      let projects = await invoke("projects_create", { name: "QB native project", client: "claude-code" });
      let project = projects.projects.find((p) => p.name === "QB native project");
      await invoke("projects_rename", { id: project.id, name: "QB native project edited" });
      await invoke("projects_clear", { client: "claude-code" });
      const applied = await invoke("projects_apply", { id: project.id, client: "claude-code" });
      check(applied.warnings.length === 0, `Project apply warnings: ${JSON.stringify(applied.warnings)}`);
      pass("Project create/rename/clear/apply through native IPC");

      const settings = await invoke("settings_load");
      const snapshot = await invoke("snapshot_create", { note: "QB native snapshot" });
      const workspace = await invoke("workspace_state");
      const provider = workspace.providers.find((p) => p.name === "QB-native-route");
      check(provider, "Provider not present before snapshot mutation");
      await invoke("provider_save", { provider: { ...provider, name: "QB native changed after snapshot" } });
      await invoke("settings_save", { next: { ...settings, gate_auto_rearm: !settings.gate_auto_rearm } });
      await invoke("snapshot_restore", { id: snapshot.id });
      check((await invoke("settings_load")).gate_auto_rearm === settings.gate_auto_rearm, "Snapshot did not restore settings");
      check((await invoke("workspace_state")).providers.some((p) => p.name === "QB-native-route"), "Snapshot did not restore provider");
      pass("Configuration snapshot creation/mutation/restore plus automatic pre-restore backup");

      const query = { text: "QB native session", content: "", kind: null, source: null, offset: 0, limit: 20 };
      let sessions = await invoke("sessions_list", query);
      check(sessions.items.length === 1, "Fixture session listing failed");
      const session = sessions.items[0];
      const detail = await invoke("sessions_read", { id: session.id, offset: 0, limit: 100 });
      check(detail.messages.some((m) => m.text.includes("QB native session")), "Session transcript read failed");
      await invoke("sessions_export", { id: session.id, path: `${config.root}/exported-session.md` });
      const trash = await invoke("sessions_delete", { id: session.id });
      check((await invoke("sessions_list", query)).items.length === 0, "Session delete failed");
      await invoke("sessions_trash_restore", { id: trash.id });
      check((await invoke("sessions_list", query)).items.length === 1, "Session restore failed");
      pass("Session list/read/Markdown export/trash/restore from local transcript fixture");

      const autostart = await invoke("autostart_set", { enabled: true });
      check(autostart.registered && autostart.points_here && autostart.effective && !autostart.dev_build, "LaunchAgent enable did not point to this packaged app");
      const disabled = await invoke("autostart_set", { enabled: false });
      check(!disabled.registered && !disabled.effective, "LaunchAgent disable did not remove registration");
      pass("Packaged app LaunchAgent enable/status/disable in disposable runner");
    } else if (config.phase === "verify") {
      check((await invoke("mcp_list")).servers.some((s) => s.id === "qb-native-mcp" && s.name.endsWith("edited")), "MCP did not survive process restart");
      check((await invoke("skills_list")).skills.some((s) => s.id === "qb-native-skill"), "Skill did not survive restart");
      check((await invoke("prompts_list")).prompts.some((p) => p.name === "QB native prompt" && p.content === "Native prompt v2"), "Prompt did not survive restart");
      check((await invoke("tool_files_view", { tool: "pi" })).entries.some((f) => f.name === "prompts/qb-native.md" && f.content === "Native tool v2"), "Tool file did not survive restart");
      check((await invoke("projects_list")).projects.some((p) => p.name === "QB native project edited"), "Project did not survive restart");
      check((await invoke("station_routes")).some((r) => r.group === "native-fixture" && r.nominal_rate === 0.75), "Route did not survive restart");
      pass("Second native process reopens encrypted MCP, Skill, prompt, tool file, project and route state");

      await invoke("plugin:event|emit", { event: "qb-macos-ui-stage", payload: "await-url" });
      const entry = await until(() => invoke("deeplink_peek"), (e) => e?.view?.kind === "prompt" && e.view.name === "QB-native-URL", "macOS Opened URL did not reach native inbox");
      await invoke("plugin:event|emit", { event: "qb-macos-ui-stage", payload: "url-inbox" });
      await until(() => [...document.querySelectorAll("dialog[open]")].some((dialog) => dialog.textContent.includes("QB-native-URL")), (ready) => ready, "Native URL did not open confirmation dialog");
      await invoke("plugin:event|emit", { event: "qb-macos-ui-stage", payload: "url-confirmation" });
      check(!(await invoke("prompts_list")).prompts.some((p) => p.name === "QB-native-URL"), "Native URL imported without confirmation");
      const confirmation = [...document.querySelectorAll("dialog[open]")].find((dialog) => dialog.textContent.includes("QB-native-URL"));
      const dismiss = [...confirmation.querySelectorAll("button")].find((b) => b.textContent.trim() === "不导入");
      check(dismiss, "URL confirmation has no dismiss action"); dismiss.click();
      await until(() => invoke("deeplink_peek"), (e) => e === null, "URL dismissal did not clear inbox");
      check(!(await invoke("prompts_list")).prompts.some((p) => p.name === "QB-native-URL"), "Dismissed native URL persisted a prompt");
      pass("Actual operating-system qbgate URL opens confirmation and does not auto-import");
      await invoke("plugin:event|emit", { event: "qb-macos-ui-stage", payload: "await-invalid" });
      await until(() => invoke("deeplink_peek"), (e) => e?.view?.kind === "error", "Unsupported native URL action was not rejected");
      pass("Unsupported native URL displays parse error without importing");
      for (const button of document.querySelectorAll("dialog[open] button")) if (button.textContent.trim() === "关闭") button.click();
      await reject("deeplink_paste", { url: "file:///tmp/qb-native-should-not-open" });
      await invoke("mcp_delete", { id: "qb-native-mcp" });
      check(!(await invoke("mcp_list")).servers.some((s) => s.id === "qb-native-mcp"), "MCP deletion failed");
      const tool = (await invoke("tool_files_view", { tool: "pi" })).entries.find((f) => f.name === "prompts/qb-native.md");
      await invoke("tool_files_delete", { tool: "pi", name: tool.name, revision: tool.revision });
      check(!(await invoke("tool_files_view", { tool: "pi" })).entries.some((f) => f.name === tool.name), "Tool file deletion failed");
      const project = (await invoke("projects_list")).projects.find((p) => p.name === "QB native project edited");
      await invoke("projects_delete", { id: project.id });
      const route = (await invoke("station_routes")).find((r) => r.group === "native-fixture");
      await invoke("station_remove_route", { id: route.id });
      pass("MCP/tool file/project/route deletion on persisted fixture state");
    } else if (config.phase === "updated") {
      check((await invoke("prompts_list")).prompts.some((p) => p.name === "QB native prompt" && p.content === "Native prompt v2"), "Replacing the app lost persistent data");
      check((await invoke("skills_list")).skills.some((s) => s.id === "qb-native-skill"), "Replacement lost installed Skill");
      check((await invoke("workspace_state")).providers.some((p) => p.name === "QB-native-route"), "Replacement lost provider data");
      pass("Replaced app starts successfully and retains native persistent data");
    }
    location.hash = "/relays";
    report.ok = true;
  } catch (error) { report.error = String(error); }
  await invoke("plugin:event|emit", { event: "qb-macos-ui-smoke", payload: report });
})();
