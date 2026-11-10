import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join, resolve } from "node:path";

assert.equal(process.platform, "darwin");
assert.equal(process.env.GITHUB_ACTIONS, "true");
const runner = realpathSync(process.env.RUNNER_TEMP);
const root = realpathSync(mkdtempSync(join(runner, "qb-st-用户 O'Brien-")));
const smokeHome = join(root, "home");
const evidence = resolve("test-results/macos-sillytavern");
for (const dir of [
  smokeHome,
  join(smokeHome, "Library", "Preferences"),
  join(smokeHome, "Library", "Keychains"),
  evidence,
])
  mkdirSync(dir, { recursive: true, mode: 0o700 });
const binary = resolve(
  process.env.CARGO_TARGET_DIR || "target",
  "debug/examples/macos-sillytavern-smoke",
);
assert.ok(existsSync(binary), "Build the native SillyTavern smoke first");
const env = {
  PATH: process.env.PATH,
  TMPDIR: process.env.TMPDIR,
  LANG: "en_US.UTF-8",
  CI: "true",
  GITHUB_ACTIONS: "true",
  RUNNER_TEMP: runner,
  HOME: smokeHome,
  npm_config_cache: join(root, "npm-cache"),
  npm_config_userconfig: join(root, "empty-npmrc"),
  QB_SILLYTAVERN_SMOKE_ROOT: root,
  QB_SILLYTAVERN_KEYCHAIN: "isolated",
};
writeFileSync(env.npm_config_userconfig, "", { mode: 0o600 });
function security(args, selectedEnv = env) {
  const result = spawnSync("/usr/bin/security", args, {
    env: selectedEnv,
    encoding: "utf8",
    timeout: 30_000,
  });
  if (result.error || result.status !== 0)
    throw new Error(
      `security ${args[0]} failed: ${result.error?.message || result.stderr}`,
    );
  return result.stdout;
}
const quoted = (text) => [...text.matchAll(/"([^"\n]+)"/g)].map((m) => m[1]);
const oldDefault = quoted(
  security(["default-keychain", "-d", "user"], process.env),
)[0];
const oldSearch = quoted(
  security(["list-keychains", "-d", "user"], process.env),
);
assert.ok(oldDefault && oldSearch.length, "Cannot preserve runner Keychains");
const keychain = join(root, "fixture.keychain-db");
const password = randomBytes(32).toString("hex");
let created = false;
let child;
let output = "";
const failures = [];
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const record = (text) => {
  output += text;
  try {
    writeFileSync(join(evidence, "smoke.log"), output);
  } catch (error) {
    failures.push(`Evidence log: ${error.message}`);
  }
  process.stdout.write(text);
};
async function terminate() {
  if (!child) return;
  const proc = child;
  if (!Number.isInteger(proc.pid)) {
    child = undefined;
    return;
  }
  for (const signal of ["SIGTERM", "SIGKILL"]) {
    if (proc.exitCode !== null || proc.signalCode !== null) break;
    try {
      process.kill(-proc.pid, signal);
    } catch (error) {
      if (error.code !== "ESRCH") throw error;
    }
    for (
      let i = 0;
      i < 40 && proc.exitCode === null && proc.signalCode === null;
      i++
    )
      await delay(100);
  }
  if (proc.exitCode === null && proc.signalCode === null)
    throw new Error("Smoke owner did not terminate");
  child = undefined;
}
try {
  security(["create-keychain", "-p", password, keychain]);
  created = true;
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
  child = spawn(binary, [], {
    env,
    detached: true,
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", (data) => record(data.toString()));
  child.stderr.on("data", (data) => record(data.toString()));
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(
      () =>
        reject(
          new Error("SillyTavern installation/start smoke exceeded 20 minutes"),
        ),
      20 * 60_000,
    );
    child.once("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
    child.once("exit", (code, signal) => {
      clearTimeout(timeout);
      if (code === 0) resolve();
      else reject(new Error(`SillyTavern smoke failed (${code}, ${signal})`));
    });
  });
} catch (error) {
  failures.push(error.message);
} finally {
  try {
    await terminate();
  } catch (error) {
    failures.push(error.message);
  }
  // The managed helper owns its own process group. Its parent-exit monitor must
  // stop that group before cleanup accepts the production process record.
  if (created) {
    let cleaned = false;
    for (let attempt = 0; attempt < 10; attempt++) {
      const result = spawnSync(binary, ["--cleanup"], {
        env,
        encoding: "utf8",
        timeout: 15_000,
      });
      if (!result.error && result.status === 0) {
        record(result.stdout);
        cleaned = true;
        break;
      }
      if (attempt === 9)
        failures.push(
          `Owned-process cleanup failed: ${result.error?.message || result.stderr}`,
        );
      await delay(500);
    }
    if (cleaned)
      record(
        "PASS wrapper cleanup confirms production process ownership was released\n",
      );
  }
  for (const file of [
    "installed.json",
    "result.json",
    "console.json",
    "running-console.json",
  ]) {
    try {
      if (existsSync(join(root, file)))
        copyFileSync(join(root, file), join(evidence, file));
    } catch (error) {
      failures.push(`Evidence ${file}: ${error.message}`);
    }
  }
  for (const selectedEnv of [env, process.env]) {
    for (const args of [
      ["default-keychain", "-d", "user", "-s", oldDefault],
      ["list-keychains", "-d", "user", "-s", ...oldSearch],
    ]) {
      try {
        security(args, selectedEnv);
      } catch (error) {
        failures.push(error.message);
      }
    }
  }
  if (created) {
    try {
      security(["delete-keychain", keychain]);
    } catch (error) {
      failures.push(error.message);
    }
  }
  try {
    assert.ok(root.startsWith(`${runner}/`) && root !== runner);
    rmSync(root, { recursive: true, force: true });
  } catch (error) {
    failures.push(error.message);
  }
  if (failures.length) record(`FAIL: ${failures.join("; ")}\n`);
}
writeFileSync(
  join(evidence, "wrapper-result.json"),
  JSON.stringify({ ok: failures.length === 0, failures }, null, 2),
);
if (failures.length) throw new Error(failures.join("; "));
