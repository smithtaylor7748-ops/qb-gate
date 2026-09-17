import { readFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import assert from "node:assert/strict";
const json = (p) => JSON.parse(readFileSync(p, "utf8"));
const pkg = json("package.json");
assert.equal(pkg.version, json("src-tauri/tauri.conf.json").version);
assert.equal(pkg.version, json("package-lock.json").version);
assert.equal(pkg.version, json("package-lock.json").packages[""].version);
const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
assert.equal(pkg.version, cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]);
// workspace 的 [workspace.package] version 是其余 crate 的版本来源。
// src-tauri 那份必须写成字面量（上面这条正则要读它），所以两处会分叉 ——
// 分叉本身没问题，无人发现的分叉才有问题，这里把它变成一道门。
const ws = readFileSync("Cargo.toml", "utf8");
assert.equal(
  pkg.version,
  ws.match(/\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/ms)?.[1],
  "Root Cargo.toml [workspace.package] version must match package.json",
);
assert.equal(pkg.license, "AGPL-3.0-or-later");
if (process.env.GITHUB_REF_TYPE === "tag")
  assert.equal(
    process.env.GITHUB_REF_NAME,
    `v${pkg.version}`,
    "Release tag must match source version",
  );
const files = [
  ...new Set(
    execFileSync("git", ["ls-files", "-co", "--exclude-standard", "-z"], {
      encoding: "utf8",
    })
      .split("\0")
      .filter(Boolean),
  ),
];
const problems = [];
for (const file of files) {
  if (!existsSync(file)) continue;
  if (
    /\.(?:sqlite3?|db)(?:-(?:wal|shm))?$|(?:^|\/)(?:auth\.json|\.credentials\.json|\.env)$/.test(
      file,
    )
  )
    problems.push(`${file}: runtime or credential file`);
  if (!/\.(?:md|rs|tsx?|m?js|json|ya?ml|toml|txt|css)$/.test(file)) continue;
  const body = readFileSync(file, "utf8");
  if (body.includes("\0")) problems.push(`${file}: embedded NUL`);
  const lines = body.split(/\r?\n/);
  lines.forEach((line, i) => {
    if (
      /(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|sk-(?:ant-|proj-)[A-Za-z0-9_-]{30,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)/.test(
        line,
      )
    )
      problems.push(`${file}:${i + 1}: possible credential`);
    const paths = line.matchAll(/[A-Z]:[\\/]+Users[\\/]+([^\\/\s"'<>]+)/gi);
    for (const [, user] of paths)
      if (
        ![
          "demo",
          "me",
          "test",
          "user",
          "username",
          "public",
          "default",
        ].includes(user.toLowerCase())
      )
        problems.push(`${file}:${i + 1}: personal user path`);
  });
}
assert.deepEqual(
  problems,
  [],
  `Release source checks failed:\n${problems.join("\n")}`,
);
console.log(
  `Version ${pkg.version} matches; ${files.length} public source files checked. No high-confidence credential or private path matches.`,
);
