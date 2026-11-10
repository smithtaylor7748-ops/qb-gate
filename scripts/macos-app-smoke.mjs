import { spawn, spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  existsSync,
  readFileSync,
  writeFileSync,
  copyFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import assert from "node:assert/strict";

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
function run(command, args) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    timeout: 60_000,
  });
  if (result.status !== 0 || result.error)
    throw new Error(`${command}: ${result.stderr || result.error}`);
  return result.stdout;
}
let child;
let mounted = false;
try {
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
  child = spawn(binary, [], {
    env: { ...process.env, HOME: smokeHome, QB_MACOS_UI_SMOKE_DIR: root },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let output = "";
  child.stdout.on("data", (data) => {
    output += data;
  });
  child.stderr.on("data", (data) => {
    output += data;
  });
  const result = join(root, "webview.json");
  for (let attempt = 0; attempt < 180 && !existsSync(result); attempt++) {
    if (child.exitCode !== null) break;
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  writeFileSync(join(evidence, "app.log"), output);
  const injectionError = join(root, "injection-error.log");
  if (existsSync(injectionError))
    copyFileSync(injectionError, join(evidence, "injection-error.log"));
  assert.ok(
    existsSync(result),
    `Packaged WKWebView did not report readiness: ${output}`,
  );
  copyFileSync(result, join(evidence, "webview.json"));
  spawnSync(
    "/usr/sbin/screencapture",
    ["-x", join(evidence, "native-window.png")],
    { timeout: 10_000 },
  );
  const report = JSON.parse(readFileSync(result, "utf8"));
  assert.equal(report.ok, true, JSON.stringify(report));
  console.log(JSON.stringify(report));
} finally {
  child?.kill("SIGTERM");
  if (mounted) run("/usr/bin/hdiutil", ["detach", mount]);
}
