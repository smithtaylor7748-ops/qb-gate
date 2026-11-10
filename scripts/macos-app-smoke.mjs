import { spawn, spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  existsSync,
  readFileSync,
  writeFileSync,
  copyFileSync,
  realpathSync,
  utimesSync,
  rmSync,
} from "node:fs";
import { join, resolve } from "node:path";
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";

if (
  process.platform !== "darwin" ||
  process.env.GITHUB_ACTIONS !== "true" ||
  !process.env.RUNNER_TEMP
) {
  throw new Error(
    "Packaged app smoke requires a disposable macOS Actions runner.",
  );
}
const root = mkdtempSync(join(process.env.RUNNER_TEMP, "qb-app-smoke-"));
const smokeHome = join(root, "home");
const state = join(smokeHome, "Library", "Application Support", "ClaudeIpGate");
mkdirSync(state, { recursive: true });
writeFileSync(
  join(state, "settings.json"),
  JSON.stringify({
    update_check_on_start: false,
    align_timezone_on_start: false,
    align_locale_on_start: false,
    align_display_language_on_start: false,
    start_router_on_launch: false,
  }),
);
const artifact = resolve("dist-installers/QB-Gate-macOS-arm64.dmg");
const mount = join(root, "mounted");
const evidence = resolve("test-results/macos-native");
mkdirSync(mount);
mkdirSync(evidence, { recursive: true });
function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    timeout: 60_000,
    ...options,
  });
  if (result.status !== 0 || result.error)
    throw new Error(`${command}: ${result.stderr || result.error}`);
  return result.stdout;
}
let child;
let mounted = false;
const keychain = join(root, "fixture.keychain-db");
const password = randomBytes(32).toString("hex");
const quoted = (text) => [...text.matchAll(/"([^"\n]+)"/g)].map((m) => m[1]);
const oldDefault = quoted(
  run("/usr/bin/security", ["default-keychain", "-d", "user"]),
)[0];
const oldSearch = quoted(
  run("/usr/bin/security", ["list-keychains", "-d", "user"]),
);
assert.ok(
  oldDefault && oldSearch.length,
  "Cannot preserve runner Keychain settings",
);
const env = { ...process.env, HOME: smokeHome, QB_MACOS_UI_SMOKE_DIR: root };
const security = (args, selectedEnv = env) =>
  run("/usr/bin/security", args, { env: selectedEnv });
let keychainCreated = false;
let updateMount;
const lifecycle = [];
const waitFor = async (check, message, seconds = 180) => {
  for (let attempt = 0; attempt < seconds * 4; attempt++) {
    if (await check()) return;
    if (existsSync(join(root, "webview.json"))) {
      const outcome = JSON.parse(
        readFileSync(join(root, "webview.json"), "utf8"),
      );
      if (!outcome.ok) throw new Error(JSON.stringify(outcome));
    }
    if (child && child.exitCode !== null)
      throw new Error(`${message}: app exited ${child.exitCode}`);
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(message);
};
const waitExit = async () => {
  const proc = child;
  for (let attempt = 0; attempt < 160 && proc.exitCode === null; attempt++)
    await new Promise((resolve) => setTimeout(resolve, 250));
  assert.equal(
    proc.exitCode,
    0,
    `Native app did not exit cleanly: ${proc.exitCode}`,
  );
  child = undefined;
};
const nativeQuit = async (app) => {
  // This sends the operating system's Quit Apple event, not Window.close().
  run("/usr/bin/osascript", [
    "-e",
    `tell application ${JSON.stringify(app)} to quit`,
  ]);
  await waitExit();
};
const start = (app, phase) => {
  for (const file of [
    "webview.json",
    "await-url",
    "await-invalid",
    "closed.json",
    "reopened.json",
  ])
    rmSync(join(root, file), { force: true });
  writeFileSync(join(root, "phase"), phase);
  let output = "";
  child = spawn(join(app, "Contents/MacOS/qb-gate"), [], {
    env,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", (data) => {
    output += data;
    writeFileSync(join(evidence, `${phase}-app.log`), output);
  });
  child.stderr.on("data", (data) => {
    output += data;
    writeFileSync(join(evidence, `${phase}-app.log`), output);
  });
};
const report = async (phase) => {
  await waitFor(
    () => existsSync(join(root, "webview.json")),
    `${phase} native IPC validation did not finish`,
  );
  const value = JSON.parse(readFileSync(join(root, "webview.json"), "utf8"));
  writeFileSync(
    join(evidence, `${phase}-webview.json`),
    JSON.stringify(value, null, 2),
  );
  assert.equal(value.ok, true, JSON.stringify(value));
  return value;
};
try {
  for (const dir of [
    "Library/Preferences",
    "Library/Keychains",
    ".claude",
    ".pi/agent",
  ])
    mkdirSync(join(smokeHome, dir), { recursive: true });
  const launchAgent = join(
    smokeHome,
    "Library/LaunchAgents/com.qbgate.panel.autostart.plist",
  );
  assert.equal(existsSync(launchAgent), false);
  const disabled = run("/bin/launchctl", [
    "print-disabled",
    `gui/${process.getuid()}`,
  ]);
  assert.ok(
    !disabled.includes('"com.qbgate.panel.autostart"'),
    "Disposable runner already has this login item override",
  );
  security(["create-keychain", "-p", password, keychain]);
  keychainCreated = true;
  security(["set-keychain-settings", "-lut", "3600", keychain]);
  security(["unlock-keychain", "-p", password, keychain]);
  security(["default-keychain", "-d", "user", "-s", keychain]);
  security(["list-keychains", "-d", "user", "-s", keychain]);
  for (const args of [
    ["default-keychain", "-d", "user"],
    ["list-keychains", "-d", "user"],
  ]) {
    const paths = quoted(security(args));
    assert.equal(paths.length, 1);
    assert.equal(realpathSync(paths[0]), realpathSync(keychain));
  }
  writeFileSync(
    join(smokeHome, ".claude.json"),
    JSON.stringify({
      mcpServers: { "unrelated-fixture": { command: "/usr/bin/true" } },
    }),
  );
  writeFileSync(
    join(smokeHome, ".claude/CLAUDE.md"),
    "Original native fixture prompt",
  );
  const skill = join(root, "skill-source", "qb-native-skill");
  mkdirSync(skill, { recursive: true });
  writeFileSync(
    join(skill, "SKILL.md"),
    "---\nname: qb-native-skill\ndescription: Disposable local native fixture\nlicense: MIT\n---\nNative fixture only.\n",
  );
  const transcript = join(
    smokeHome,
    ".claude/projects/qb-native/qb-native-session.jsonl",
  );
  mkdirSync(join(smokeHome, ".claude/projects/qb-native"), { recursive: true });
  writeFileSync(
    transcript,
    [
      {
        type: "user",
        cwd: root,
        timestamp: "2026-11-01T00:00:00Z",
        message: { role: "user", content: "QB native session fixture" },
      },
      {
        type: "assistant",
        timestamp: "2026-11-01T00:00:01Z",
        message: {
          role: "assistant",
          content: [{ type: "text", text: "Local fixture response" }],
        },
      },
    ]
      .map((line) => JSON.stringify(line))
      .join("\n") + "\n",
  );
  const age = new Date(Date.now() - 600_000);
  utimesSync(transcript, age, age);
  run("/usr/bin/hdiutil", [
    "attach",
    artifact,
    "-readonly",
    "-nobrowse",
    "-mountpoint",
    mount,
  ]);
  mounted = true;
  const app = join(root, "QB Gate.app");
  run("/usr/bin/ditto", [join(mount, "QB Gate.app"), app]);
  run("/usr/bin/codesign", ["--verify", "--deep", "--strict", app]);
  const entitlements = run("/usr/bin/codesign", [
    "-d",
    "--entitlements",
    ":-",
    app,
  ]);
  assert.match(entitlements, /com\.apple\.security\.automation\.apple-events/);
  assert.match(entitlements, /<true\s*\/>/);
  writeFileSync(join(evidence, "entitlements.plist"), entitlements);
  const usage = run("/usr/libexec/PlistBuddy", [
    "-c",
    "Print :NSAppleEventsUsageDescription",
    join(app, "Contents/Info.plist"),
  ]).trim();
  assert.ok(usage.includes("终端"), "Missing Terminal automation purpose");
  const binary = join(app, "Contents/MacOS/qb-gate");
  assert.equal(run("/usr/bin/lipo", ["-archs", binary]).trim(), "arm64");
  assert.equal(
    run("/usr/libexec/PlistBuddy", [
      "-c",
      "Print :LSMinimumSystemVersion",
      join(app, "Contents/Info.plist"),
    ]).trim(),
    "13.3",
  );
  run("/usr/bin/hdiutil", ["detach", mount]);
  mounted = false;
  start(app, "create");
  await report("create");
  assert.equal(existsSync(launchAgent), false, "LaunchAgent was not removed");
  const mcp = JSON.parse(
    readFileSync(join(smokeHome, ".claude.json"), "utf8"),
  ).mcpServers;
  assert.ok(
    mcp["qb-native-mcp"] && mcp["unrelated-fixture"],
    "MCP target writes lost unrelated configuration",
  );
  assert.equal(
    readFileSync(join(smokeHome, ".claude/CLAUDE.md"), "utf8"),
    "Native prompt v2",
  );
  assert.ok(
    existsSync(join(smokeHome, ".claude/skills/qb-native-skill/SKILL.md")),
  );
  assert.match(
    readFileSync(join(root, "exported-session.md"), "utf8"),
    /QB native session fixture/,
  );
  lifecycle.push(
    "Native IPC writes verified on disk; unrelated MCP configuration preserved",
  );
  writeFileSync(join(root, "control"), "close");
  await waitFor(
    () => existsSync(join(root, "closed.json")),
    "Window close was not observed",
    15,
  );
  const closed = JSON.parse(readFileSync(join(root, "closed.json"), "utf8"));
  assert.ok(
    closed.tray && !closed.visible,
    "Close did not retain a hidden app with tray",
  );
  run("/usr/bin/open", ["-a", app]);
  await waitFor(
    () => existsSync(join(root, "reopened.json")),
    "macOS reopen event was not received",
    20,
  );
  assert.equal(
    JSON.parse(readFileSync(join(root, "reopened.json"), "utf8")).visible,
    true,
  );
  lifecycle.push(
    "Real window close hides to menu bar; operating-system reopen reveals the app",
  );
  await nativeQuit(app);
  lifecycle.push(
    "Operating-system Quit Apple event exits the native app cleanly",
  );
  start(app, "verify");
  await waitFor(
    () => existsSync(join(root, "await-url")),
    "App was not ready for real URL event",
  );
  run("/usr/bin/open", [
    "-a",
    app,
    "qbgate://v1/import?resource=prompt&app=codex&name=QB-native-URL&content=TmF0aXZlIFVSTCBmaXh0dXJl",
  ]);
  await waitFor(
    () => existsSync(join(root, "await-invalid")),
    "Native URL confirmation did not complete",
    100,
  );
  run("/usr/bin/open", [
    "-a",
    app,
    "qbgate://v1/unsupported?resource=prompt&app=codex&name=Rejected",
  ]);
  await report("verify");
  const remainingMcp = JSON.parse(
    readFileSync(join(smokeHome, ".claude.json"), "utf8"),
  ).mcpServers;
  assert.ok(
    !remainingMcp["qb-native-mcp"] && remainingMcp["unrelated-fixture"],
  );
  lifecycle.push(
    "Native app restart retains persisted fixtures and safely accepts actual macOS URL events",
  );
  const packageDir = join(root, "package");
  mkdirSync(packageDir);
  copyFileSync(artifact, join(packageDir, "QB-Gate-macOS-arm64.dmg"));
  copyFileSync(
    resolve("dist-installers/SHA256SUMS.txt"),
    join(packageDir, "SHA256SUMS.txt"),
  );
  writeFileSync(join(root, "control"), "update");
  await waitExit();
  const prepared = JSON.parse(
    readFileSync(join(root, "update-prepared.json"), "utf8"),
  );
  assert.equal(prepared.ok, true, JSON.stringify(prepared));
  const updateImage = realpathSync(join(packageDir, "QB-Gate-macOS-arm64.dmg"));
  await waitFor(
    () => {
      const info = JSON.parse(
        run("/usr/bin/plutil", ["-convert", "json", "-o", "-", "-"], {
          input: run("/usr/bin/hdiutil", ["info", "-plist"]),
        }),
      );
      const disk = info.images.find(
        (image) =>
          image["image-path"] &&
          realpathSync(image["image-path"]) === updateImage,
      );
      updateMount = disk?.["system-entities"]?.find(
        (entity) => entity["mount-point"],
      )?.["mount-point"];
      return !!updateMount;
    },
    "Update exit handler did not open and mount the verified DMG",
    60,
  );
  const replacement = join(root, "replacement", "QB Gate.app");
  run("/usr/bin/ditto", [join(updateMount, "QB Gate.app"), replacement]);
  run("/usr/bin/codesign", ["--verify", "--deep", "--strict", replacement]);
  lifecycle.push(
    "Production update exit handler rechecks selected arm64 DMG hash and opens it; app copied from mounted image",
  );
  start(replacement, "updated");
  const final = await report("updated");
  copyFileSync(join(root, "webview.json"), join(evidence, "webview.json"));
  spawnSync(
    "/usr/sbin/screencapture",
    ["-x", join(evidence, "native-window.png")],
    { timeout: 10_000 },
  );
  await nativeQuit(replacement);
  lifecycle.push(
    "Replacement app relaunch preserves encrypted workspace data and quits normally",
  );
  writeFileSync(
    join(evidence, "lifecycle.json"),
    JSON.stringify(
      {
        ok: true,
        checks: lifecycle,
        update_scope:
          "Same-version real DMG: production hash selection and exit handoff, disk-image mount, manual-copy replacement and relaunch. Online release download and newer-version migration not exercised.",
      },
      null,
      2,
    ),
  );
  console.log(JSON.stringify(final));
} finally {
  const failures = [];
  const cleanup = (action) => {
    try {
      action();
    } catch (error) {
      failures.push(error.message);
    }
  };
  if (child && child.exitCode === null) {
    child.kill("SIGTERM");
    for (let attempt = 0; attempt < 40 && child.exitCode === null; attempt++)
      await new Promise((resolve) => setTimeout(resolve, 250));
    if (child.exitCode === null) {
      child.kill("SIGKILL");
      for (let attempt = 0; attempt < 20 && child.exitCode === null; attempt++)
        await new Promise((resolve) => setTimeout(resolve, 250));
    }
    if (child.exitCode === null && child.signalCode === null)
      failures.push("Native app cleanup did not complete");
  }
  for (const file of [
    "webview.json",
    "injection-error.log",
    "closed.json",
    "reopened.json",
    "await-url",
    "url-inbox",
    "url-confirmation",
    "await-invalid",
  ])
    if (existsSync(join(root, file)))
      cleanup(() =>
        copyFileSync(
          join(root, file),
          join(evidence, file === "webview.json" ? "last-webview.json" : file),
        ),
      );
  if (mounted) cleanup(() => run("/usr/bin/hdiutil", ["detach", mount]));
  if (updateMount)
    cleanup(() => run("/usr/bin/hdiutil", ["detach", updateMount]));
  cleanup(() =>
    rmSync(
      join(smokeHome, "Library/LaunchAgents/com.qbgate.panel.autostart.plist"),
      { force: true },
    ),
  );
  for (const selectedEnv of [env, process.env]) {
    for (const args of [
      ["default-keychain", "-d", "user", "-s", oldDefault],
      ["list-keychains", "-d", "user", "-s", ...oldSearch],
    ])
      cleanup(() => security(args, selectedEnv));
  }
  if (keychainCreated) cleanup(() => security(["delete-keychain", keychain]));
  if (failures.length)
    throw new Error(`Native smoke cleanup failed: ${failures.join("; ")}`);
}
