import { describe, expect, it } from "vitest";

import { bridgeBlockers, type BridgeReadiness } from "./tavernReady";

const READY: BridgeReadiness = {
  tavernPathsUnset: false,
  cli: "C:\\Program Files\\nodejs\\node.exe bundle\\gemini.js",
  cliLabel: "官方 Gemini CLI",
  cliWhere: "到「软件」页点「用 npm 安装」",
  slot: "个人",
  slotLoggedIn: true,
  slotWhere: "「官方账户 · 反重力」",
};

describe("bridgeBlockers", () => {
  it("样样齐了就一条都不报", () => {
    expect(bridgeBlockers(READY)).toEqual([]);
  });

  it("没有激活槽位时说得出该去哪儿建", () => {
    // 2026-09-21 使用者报的就是这一条：面板上没有任何 Gemini 槽位，
    // 而「起 Gemini 酒馆」看上去是可以点的，点下去才拿到后端那句话。
    const out = bridgeBlockers({ ...READY, slot: null });
    expect(out).toHaveLength(1);
    expect(out[0]).toContain("官方账户 · 反重力");
  });

  it("有槽位但没登录，跟没有槽位是两句不同的话", () => {
    const none = bridgeBlockers({ ...READY, slot: null });
    const out = bridgeBlockers({ ...READY, slotLoggedIn: false });
    expect(out).toHaveLength(1);
    expect(out[0]).toContain("个人");
    expect(out[0]).not.toEqual(none[0]);
  });

  it("CLI 没装时点名是哪一个 CLI、该去哪儿装", () => {
    const out = bridgeBlockers({ ...READY, cli: null });
    expect(out).toHaveLength(1);
    expect(out[0]).toContain("官方 Gemini CLI");
    expect(out[0]).toContain("软件");
  });

  it("顺序 = 该先做哪一件：路径 → CLI → 槽位", () => {
    // 倒过来做要返工：没有 CLI 就没法在槽位里完成登录。
    const out = bridgeBlockers({
      tavernPathsUnset: true,
      cli: null,
      cliLabel: "官方 Codex CLI",
      cliWhere: "到「软件」页装 Codex",
      slot: null,
      slotLoggedIn: false,
      slotWhere: "「官方账户 · GPT」",
    });
    expect(out).toHaveLength(3);
    expect(out[0]).toContain("酒馆路径");
    expect(out[1]).toContain("Codex CLI");
    expect(out[2]).toContain("账户槽位");
  });
});
