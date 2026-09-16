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

// 两个 Rust 类型导出成同一个 TS 名字时,ts-rs 后写的把先写的**盖掉而且不报错**。
//
// ⚠ 上面那条断言看不见这种事:它比的是「生成的和仓库里的一不一样」,
// 而两次生成都一样地错。实测过一次 —— `qb-station` 的 `audit::Check` 撞上
// `qb-probe` 的 `verdict::Check`,`AuditRound.checks` 被标成
// `Array<"Pass"|"Fail"|"Unknown">`,整条流水线全绿。
//
// 撞名了就在那个类型上加 `#[ts(export, rename = "…")]`,别去改 Rust 那边的名字:
// 模块里叫 `Check` 是对的,重名只是 TS 那边没有命名空间。
const declared = new Map();
const walk = (d) => {
  for (const e of readdirSync(d, { withFileTypes: true })) {
    const p = `${d}/${e.name}`;
    if (e.isDirectory()) walk(p);
    else if (e.name.endsWith(".rs")) scan(p);
  }
};
const scan = (file) => {
  const src = readFileSync(file, "utf8");
  // `#[ts(...)]` 里带 export 的,再往下找第一个 `pub struct/enum 名字`。
  const re =
    /#\[ts\(([^\]]*)\)\][\s\S]{0,400}?\bpub\s+(?:struct|enum)\s+([A-Za-z0-9_]+)/g;
  for (const [, attrs, rustName] of src.matchAll(re)) {
    if (!/\bexport\b/.test(attrs)) continue;
    const renamed = attrs.match(/rename\s*=\s*"([^"]+)"/);
    const tsName = renamed ? renamed[1] : rustName;
    const where = `${file}::${rustName}`;
    const prev = declared.get(tsName);
    if (prev && prev !== where) {
      assert.fail(
        `两个 Rust 类型都会导出成 ${tsName}.ts —— 后写的会把先写的悄悄盖掉：\n` +
          `  ${prev}\n  ${where}\n` +
          `给其中一个加 #[ts(export, rename = "…")]。`,
      );
    }
    declared.set(tsName, where);
  }
};
for (const root of ["crates", "src-tauri/src"]) walk(root);

// 上面两条都看不见「新加了 ts-rs 类型，但忘了往 export-types.rs 里添一行」。
//
// ⚠ 第一条断言比的是「重新生成的和仓库里的一不一样」—— 一个从来没被导出过的
// 类型，两边都没有它，比什么都一样。`export-types.rs` 的注释里原来写着
// 「忘了添的后果是 types:check 会当场红」，那句话对**改字段**成立，对
// **新类型**不成立：实测漏掉 StationModel / StationModelsView 两个，全绿。
//
// 后果是前端只能手抄一份形状，而手抄的那份不会跟着 Rust 一起变 ——
// 正是 B1 花了一整轮才消掉的那类盲区。
const generated = new Set(
  readdirSync(dir)
    .filter((f) => f.endsWith(".ts"))
    .map((f) => f.slice(0, -3)),
);
const missing = [...declared]
  .filter(([ts]) => !generated.has(ts))
  .map(([ts, where]) => `  ${ts} (${where})`);
assert.equal(
  missing.length,
  0,
  `这些类型标了 #[ts(export)]，但 src/lib/generated 里没有它们 —— ` +
    `多半是忘了往 src-tauri/examples/export-types.rs 里添一行：\n${missing.join("\n")}`,
);

console.log(
  `Generated IPC types match Rust contracts; ${declared.size} exported type names are unique, ` +
    `all present in ${dir}.`,
);
