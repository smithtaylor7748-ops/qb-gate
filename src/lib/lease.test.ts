import { describe, expect, it } from "vitest";

import { describeLease, holderNames, shortHolder } from "./lease";

describe("租约持有者的显示名", () => {
  it("会话 id 只留时间戳那一段", () => {
    expect(shortHolder("20260913-045632-ef2e6a0958ef48deb76f0b4d12ca77b1")).toBe(
      "20260913-045632",
    );
  });

  it("不是会话 id 的名字原样保留", () => {
    expect(shortHolder("claude-desktop")).toBe("claude-desktop");
    expect(shortHolder("manual")).toBe("manual");
  });

  /**
   * 这一条钉的是原始报告：五个会话加一个桌面端，`lease.holder` 拼出来
   * 三百多个字符，塞进四分之一屏宽的指标格里会横着溢出卡片。
   * 一行放得下才算修好 —— 不是「能换行」就行。
   */
  it("持有者多起来时只说一个加个数，不把整串铺出来", () => {
    const ids = [
      "20260912-171001-a6f57f9593984347b8bf3206c4fd8f21",
      "20260913-043119-d0406fabe8dc440da95322ebc6a1b7d3",
      "20260913-044134-51ed2114d4ad4b8f8b91fd6e9911aa02",
      "20260913-044449-f0a703d3976f422f8888b2ed3a2cc194",
      "20260913-045632-ef2e6a0958ef48deb76f0b4d12ca77b1",
      "claude-desktop",
    ];
    const out = describeLease(
      { holder: ids.join("、"), holders: Object.fromEntries(ids.map((i) => [i, "Cli"])) },
      "已租给",
    );
    expect(out?.text).toBe("已租给 20260912-171001 等 6 个");
    expect(out!.text.length).toBeLessThan(32);
    // 完整清单没有丢，只是挪到了 title 上。
    expect(out?.title.split("\n")).toEqual(ids);
  });

  it("一两个持有者时把名字说全", () => {
    expect(describeLease({ holders: { "claude-desktop": "Desktop" } }, "已租给")?.text).toBe(
      "已租给 claude-desktop",
    );
    expect(
      describeLease({ holders: { official: "Cli", relay: "Cli" } }, "已放行给")?.text,
    ).toBe("已放行给 official、relay");
  });

  /** 旧版本写下的 `lease.json` 里只有拼好的 `holder`，没有 `holders`。 */
  it("只有旧字段时按「、」拆回来", () => {
    expect(holderNames({ holder: "official、relay" })).toEqual([
      "official",
      "relay",
    ]);
  });

  it("没有持有者时返回 null，由调用方决定说什么", () => {
    expect(describeLease({ holder: null, holders: {} }, "已租给")).toBeNull();
  });
});
