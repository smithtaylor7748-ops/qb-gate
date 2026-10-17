/**
 * 样式里引用的每一个 CSS 变量都得有人定义（2026-09-29）。
 *
 * 那天重构酒馆的桥接弹窗时发现：`components.css` 里那几段写的是 `var(--line)`、`var(--line-soft)`、
 * `var(--surface-1)`、`var(--text-1)` —— `tokens.css` 里**一个都没有**。浏览器对没定义的变量不报错，
 * 那一条声明在计算时作废：边框没了、底色透明、文字颜色退回继承的。账户页酒馆磁贴右上角那颗小按钮
 * （那时开桥接设置，第三轮起开「酒馆」弹窗）因此一直是透明的，而 `test:ui` 的令牌比对只看 `:root` 上定义了什么，看不见「用了没定义的」。
 *
 * 数法：所有 `.css` 里 `--名字:` 的定义（含 Tailwind 主题块、组件自己设的局部变量）∪ TSX 里 `style={{ "--x": … }}`
 * 设的，对上所有 `var(--名字` 的引用。带回退值的 `var(--x, …)` 也算引用 —— 回退值只是遮住了拼错的名字。
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const SRC = fileURLToPath(new URL("..", import.meta.url));

function walk(dir: string, exts: RegExp, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) {
      if (name !== "generated" && name !== "node_modules") walk(p, exts, out);
    } else if (exts.test(name)) {
      out.push(p);
    }
  }
  return out;
}

describe("CSS variables", () => {
  it("every var(--x) used in a stylesheet is defined somewhere", () => {
    const css = walk(SRC, /\.css$/).map(
      (f) => [f, readFileSync(f, "utf8")] as const,
    );
    const tsx = walk(SRC, /\.tsx?$/).map((f) => readFileSync(f, "utf8"));
    const defined = new Set<string>();
    for (const [, text] of css)
      for (const m of text.matchAll(/(--[a-zA-Z0-9-]+)\s*:/g))
        defined.add(m[1]);
    // 组件里用 style 设的局部变量（`"--bridge-accent": …`）。
    for (const text of tsx)
      for (const m of text.matchAll(/["'](--[a-zA-Z0-9-]+)["']\s*:/g))
        defined.add(m[1]);

    const missing: string[] = [];
    for (const [file, text] of css) {
      for (const m of text.matchAll(/var\(\s*(--[a-zA-Z0-9-]+)/g)) {
        if (!defined.has(m[1]))
          missing.push(`${file.slice(SRC.length)}: ${m[1]}`);
      }
    }
    expect([...new Set(missing)]).toEqual([]);
    // 先证明这条断言不是空的：一个定义都没数到的话，上面那条恒绿。
    expect(defined.has("--border")).toBe(true);
    expect(defined.size).toBeGreaterThan(20);
  });
});
