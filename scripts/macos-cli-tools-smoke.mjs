import { mkdtempSync, mkdirSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { spawnSync } from "node:child_process";

if (process.platform !== "darwin" || process.env.GITHUB_ACTIONS !== "true") {
  throw new Error("Requires a disposable GitHub macOS runner");
}
const tool = process.argv[2];
if (!["opencode", "grok", "pi", "mcode", "openclaw"].includes(tool)) {
  throw new Error("Expected opencode, grok, pi, mcode, or openclaw");
}
const root = mkdtempSync(join(tmpdir(), `QB CLI 用户 O'Brien ${tool}-`));
mkdirSync(join(root, "tmp"));
const env = {
  PATH: process.env.PATH,
  HOME: root,
  TMPDIR: join(root, "tmp"),
  GITHUB_ACTIONS: "true",
  CI: "true",
  npm_config_cache: join(root, "npm-cache"),
  npm_config_userconfig: join(root, "empty.npmrc"),
  npm_config_globalconfig: join(root, "empty-global.npmrc"),
  npm_config_update_notifier: "false",
};
let status = 1;
try {
  const result = spawnSync(
    resolve("target/debug/examples/macos-cli-tools-smoke"),
    [tool, root],
    {
      env,
      stdio: "inherit",
      timeout: 15 * 60_000,
    },
  );
  if (result.error) throw result.error;
  status = result.status ?? 1;
} finally {
  rmSync(root, { recursive: true, force: true });
}
process.exitCode = status;
