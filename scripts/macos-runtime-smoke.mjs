import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { randomBytes } from "node:crypto";

if (process.platform !== "darwin" || process.env.GITHUB_ACTIONS !== "true") {
  throw new Error(
    "Native runtime smoke requires a disposable macOS Actions runner.",
  );
}
function run(program, args, options = {}) {
  const result = spawnSync(program, args, {
    encoding: "utf8",
    timeout: 240_000,
    ...options,
  });
  if (result.error || result.status !== 0) {
    throw new Error(
      `${program} failed (${result.status}): ${result.error?.message || result.stderr}`,
    );
  }
  return result.stdout;
}
const security = (args) => run("/usr/bin/security", args);
const quotedPaths = (text) =>
  [...text.matchAll(/"([^"\n]+)"/g)].map((match) => match[1]);
const oldDefault = quotedPaths(security(["default-keychain", "-d", "user"]))[0];
const oldSearch = quotedPaths(security(["list-keychains", "-d", "user"]));
if (!oldDefault || !oldSearch.length)
  throw new Error("Cannot preserve original Keychain configuration.");
const root = mkdtempSync(join(tmpdir(), "qb-macos-runtime-"));
const keychain = join(root, "fixture.keychain-db");
const smokeHome = join(root, "home");
const logPath = resolve("docs/macos-runtime-smoke.log");
const password = randomBytes(32).toString("hex");
mkdirSync(smokeHome, { recursive: true });
let created = false;
try {
  security(["create-keychain", "-p", password, keychain]);
  created = true;
  security(["set-keychain-settings", "-lut", "3600", keychain]);
  security(["unlock-keychain", "-p", password, keychain]);
  security(["default-keychain", "-d", "user", "-s", keychain]);
  security(["list-keychains", "-d", "user", "-s", keychain]);
  const env = {
    ...process.env,
    HOME: smokeHome,
    QB_MACOS_SMOKE_ROOT: root,
    QB_MACOS_SMOKE_KEYCHAIN: "isolated",
  };
  const binary = resolve("target/debug/examples/macos-runtime-smoke");
  const result = spawnSync(binary, [], {
    env,
    encoding: "utf8",
    timeout: 240_000,
  });
  const output = `${result.stdout || ""}${result.stderr || ""}`;
  writeFileSync(logPath, output, "utf8");
  process.stdout.write(output);
  if (result.error || result.status !== 0) {
    throw new Error(
      `Native runtime smoke failed (${result.status}): ${result.error?.message || "see smoke log"}`,
    );
  }
} finally {
  // Restore even after native crashes; each action is attempted independently.
  const failures = [];
  for (const args of [
    ["default-keychain", "-d", "user", "-s", oldDefault],
    ["list-keychains", "-d", "user", "-s", ...oldSearch],
    ...(created ? [["delete-keychain", keychain]] : []),
  ]) {
    try {
      security(args);
    } catch (error) {
      failures.push(error.message);
    }
  }
  rmSync(root, { recursive: true, force: true });
  if (failures.length)
    throw new Error(`Keychain cleanup failed: ${failures.join("; ")}`);
}
