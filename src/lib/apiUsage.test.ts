/**
 * 每一个命令包装都得有人调。
 *
 * 2026-09-27 清出来一批「`api.ts` 里写着、界面上没人调」的包装：`gateOpen`、`probePurity`、
 * 五个 `profile*`、`tzApply`、`routerStart` / `routerStop`、`autoUpdateSet`、`accountRename`……
 * 页面删了、按钮删了，包装留着，后端命令也就一直挂在 IPC 上。Rust 那边的
 * `every_registered_command_is_called_by_the_frontend` 只看得见「前端提到过这个命令名」——
 * 包装里提到的也算，所以那一半得在这里守：包装本身要有调用方。
 *
 * 怎么数：运行时 `Object.keys()` 拿成员（不从源码猜），再到业务代码里找
 * `对象.成员`（容许 Prettier 折行出来的 `api\n  .updateStatus()`）。定义处写的是
 * `成员: () => …`，不会自己匹配上，所以同一个文件里的 `res(workspaceApi.state)` 照样算数；
 * 单测、演示夹具、生成的类型不算调用方。
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { api } from "./api";
import { antigravityApi } from "./antigravity";
import { codexApi } from "./codexAccounts";
import { stationApi } from "./station";
import { workspaceApi } from "./workspace";

const SRC = fileURLToPath(new URL("..", import.meta.url));

function walk(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name !== "generated") walk(p, out);
    } else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name)) {
      out.push(p);
    }
  }
  return out;
}

const FIXTURES = new Set([
  "demo.ts",
  "demoUsage.ts",
  "demoTavern.ts",
  "workspaceDemo.ts",
]);
const files = walk(SRC)
  .filter((p) => !FIXTURES.has(p.split(sep).pop() ?? ""))
  .map((p) => ({
    path: relative(SRC, p).split(sep).join("/"),
    text: readFileSync(p, "utf8"),
  }));

/**
 * 按名字动态调的成员：静态搜不到 `codexApi.switch`，但真的有人调。
 * 每一条都要写明调用点，下面还会核对那个调用点还在 —— 调用点没了，这条豁免也得跟着删。
 */
const DYNAMIC: Record<
  string,
  { members: string[]; site: string; pattern: string }
> = {
  codexApi: {
    // GPT 页「切换 / 移除」确认框：`await codexApi[ask.action](ask.id)`。
    members: ["switch", "archive"],
    site: "features/overview/GptBand.tsx",
    pattern: "codexApi[ask.action]",
  },
};

const OBJECTS: [string, Record<string, unknown>][] = [
  ["api", api],
  ["workspaceApi", workspaceApi],
  ["stationApi", stationApi],
  ["codexApi", codexApi],
  ["antigravityApi", antigravityApi],
];

describe("命令包装", () => {
  it("扫得到业务代码（扫描本身没坏）", () => {
    expect(files.length).toBeGreaterThan(50);
    expect(files.some((f) => f.path === "features/Shell.tsx")).toBe(true);
    expect(files.some((f) => f.path.startsWith("lib/generated/"))).toBe(false);
  });

  it.each(OBJECTS)("%s 的每个成员都有调用方", (name, obj) => {
    const members = Object.keys(obj);
    expect(members.length).toBeGreaterThan(1);
    const dynamic = DYNAMIC[name]?.members ?? [];
    const unused = members.filter((m) => {
      if (dynamic.includes(m)) return false;
      const re = new RegExp(`\\b${name}\\s*\\??\\.\\s*${m}\\b`);
      return !files.some((f) => re.test(f.text));
    });
    expect(unused, `这些包装没人调：删掉它，连同后端那条命令`).toEqual([]);
  });

  it("动态调用的豁免还对得上调用点", () => {
    for (const [name, { members, site, pattern }] of Object.entries(DYNAMIC)) {
      const obj = OBJECTS.find(([n]) => n === name)?.[1];
      expect(obj, name).toBeDefined();
      for (const m of members) expect(Object.keys(obj!)).toContain(m);
      const file = files.find((f) => f.path === site);
      expect(file?.text, `${site} 里找不到 ${pattern}`).toContain(pattern);
    }
  });
});
