import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { join, resolve } from "node:path";

if (process.platform !== "darwin") {
  throw new Error(
    "macOS 构建需要安装 Xcode Command Line Tools 的 Mac；Windows 不能生成原生 .app/.dmg。",
  );
}
const target = "aarch64-apple-darwin";
function run(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw new Error(`${command} exited ${result.status}`);
}
run("rustup", ["target", "add", target]);
run("npm", [
  "run",
  "tauri",
  "--",
  "build",
  "--target",
  target,
  "--",
  "--locked",
]);
const bundle = join(
  resolve(process.env.CARGO_TARGET_DIR || "target"),
  target,
  "release",
  "bundle",
);
const out = "dist-installers";
mkdirSync(out, { recursive: true });
const images = readdirSync(join(bundle, "dmg")).filter((file) =>
  file.endsWith(".dmg"),
);
if (images.length !== 1)
  throw new Error(`Expected exactly one .dmg, found ${images.length}`);
const diskImage = "QB-Gate-macOS-arm64.dmg";
copyFileSync(join(bundle, "dmg", images[0]), join(out, diskImage));
const app = join(bundle, "macos", "QB Gate.app");
if (!existsSync(app)) throw new Error("Tauri did not produce QB Gate.app");
const archive = "QB-Gate-macOS-arm64.app.tar.gz";
run("tar", [
  "-czf",
  resolve(out, archive),
  "-C",
  join(bundle, "macos"),
  "QB Gate.app",
]);
writeFileSync(
  join(out, "SHA256SUMS.txt"),
  [diskImage, archive]
    .map(
      (file) =>
        `${createHash("sha256")
          .update(readFileSync(join(out, file)))
          .digest("hex")}  ${file}`,
    )
    .join("\n") + "\n",
);
for (const file of [
  "LICENSE",
  "LICENSE-ADDITIONAL-TERMS.md",
  "LICENSE-COMMERCIAL.md",
  "THIRD_PARTY_NOTICES.txt",
])
  copyFileSync(file, join(out, file));
copyFileSync("docs/dependencies.json", join(out, "dependencies.json"));
copyFileSync("docs/MACOS.zh-CN.md", join(out, "macOS-安装说明.md"));
console.log(
  `Apple Silicon app and disk image are in ${resolve(out)}. Ad-hoc signed; not notarized.`,
);
