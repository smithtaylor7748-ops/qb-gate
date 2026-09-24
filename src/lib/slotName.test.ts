import { describe, expect, it } from "vitest";

import { slotName } from "./slotName";

describe("slotName", () => {
  it("邮箱在前、命名在后", () => {
    expect(slotName("someone@example.com", "NEW1")).toBe(
      "someone@example.com - NEW1",
    );
  });

  // 没登录、官方客户端还没写档案、或者换了格式都会走到这一档。
  // 显示成「 - NEW1」比不改还糟，所以空值一律退回只显示命名。
  it("读不到邮箱就只剩命名", () => {
    expect(slotName(null, "NEW1")).toBe("NEW1");
    expect(slotName(undefined, "NEW1")).toBe("NEW1");
    expect(slotName("", "NEW1")).toBe("NEW1");
    expect(slotName("   ", "NEW1")).toBe("NEW1");
  });

  it("邮箱两头的空白不带进显示", () => {
    expect(slotName("  a@b.c  ", "main")).toBe("a@b.c - main");
  });

  // Rust 侧 `qb_accounts::accounts::display_name` 是同一套语义（托盘菜单用的那份），
  // 两边改要一起改。
  it("命名为空时不拼出一个只有分隔符的名字", () => {
    expect(slotName(null, "")).toBe("");
  });
});
