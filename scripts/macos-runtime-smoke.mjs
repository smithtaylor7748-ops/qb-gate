import { spawnSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
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
const security = (args, env = process.env) =>
  run("/usr/bin/security", args, { env });
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
mkdirSync(join(smokeHome, "Library", "Preferences"), { recursive: true });
mkdirSync(join(smokeHome, "Library", "Keychains"), { recursive: true });
const env = {
  ...process.env,
  HOME: smokeHome,
  QB_MACOS_SMOKE_ROOT: root,
  QB_MACOS_SMOKE_KEYCHAIN: "isolated",
};
const evidence = [];
let created = false;
try {
  // Security resolves per-user Keychain preferences from HOME. Configure them in
  // the same environment as the native process, including its managed helpers.
  security(["create-keychain", "-p", password, keychain], env);
  created = true;
  security(["set-keychain-settings", "-lut", "3600", keychain], env);
  security(["unlock-keychain", "-p", password, keychain], env);
  security(["default-keychain", "-d", "user", "-s", keychain], env);
  security(["list-keychains", "-d", "user", "-s", keychain], env);
  const selected = quotedPaths(
    security(["default-keychain", "-d", "user"], env),
  );
  const searched = quotedPaths(security(["list-keychains", "-d", "user"], env));
  for (const paths of [selected, searched]) {
    if (
      paths.length !== 1 ||
      realpathSync(paths[0]) !== realpathSync(keychain)
    ) {
      throw new Error(
        "Isolated HOME did not select only the temporary Keychain.",
      );
    }
  }
  evidence.push("PASS isolated HOME selects only its temporary Keychain\n");
  const binary = resolve(
    process.env.CARGO_TARGET_DIR || "target",
    "debug/examples/macos-runtime-smoke",
  );
  const result = spawnSync(binary, [], {
    env,
    encoding: "utf8",
    timeout: 240_000,
  });
  const output = `${result.stdout || ""}${result.stderr || ""}`;
  evidence.push(output);
  process.stdout.write(output);
  if (result.error || result.status !== 0) {
    throw new Error(
      `Native runtime smoke failed (${result.status}): ${result.error?.message || "see smoke log"}`,
    );
  }
} catch (error) {
  evidence.push(`${error.message}\n`);
  throw error;
} finally {
  // Restore even after native crashes; each action is attempted independently.
  const failures = [];
  for (const restoreEnv of [env, process.env]) {
    for (const args of [
      ["default-keychain", "-d", "user", "-s", oldDefault],
      ["list-keychains", "-d", "user", "-s", ...oldSearch],
    ]) {
      try {
        security(args, restoreEnv);
      } catch (error) {
        failures.push(error.message);
      }
    }
  }
  if (created) {
    try {
      security(["delete-keychain", keychain], env);
    } catch (error) {
      failures.push(error.message);
    }
  }
  rmSync(root, { recursive: true, force: true });
  if (failures.length)
    evidence.push(`Keychain cleanup failed: ${failures.join("; ")}\n`);
  writeFileSync(logPath, evidence.join(""), "utf8");
  if (failures.length)
    throw new Error(`Keychain cleanup failed: ${failures.join("; ")}`);
}
