import { readFileSync, readdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import assert from "node:assert/strict";
const dir = "src/lib/generated";
const read = () =>
  Object.fromEntries(
    readdirSync(dir)
      .sort()
      .map((f) => [
        f,
        readFileSync(`${dir}/${f}`, "utf8").replaceAll("\r\n", "\n"),
      ]),
  );
const before = read();
execFileSync(
  "cargo",
  [
    "run",
    "--locked",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--example",
    "export-types",
    "--quiet",
  ],
  { stdio: "inherit" },
);
assert.deepEqual(
  read(),
  before,
  "Rust IPC types changed; run npm run types:generate and include the generated files.",
);
console.log("Generated IPC types match Rust contracts.");
