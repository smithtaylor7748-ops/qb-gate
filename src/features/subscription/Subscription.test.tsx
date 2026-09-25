// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import Subscription from "./Subscription";
import { ACK_KEY } from "./AckModal";
import { FAQ_LIST, PLANS, RELAY_DOWNSIDES, SOURCES } from "./data";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

/**
 * 使用者定的口径：页面上不出现特定国家、地区、发卡行与已删掉的卡商。
 * 这条测试盯着**渲染出来的文字**，不是源码 —— 文案改回去当场红。
 * 唯一的例外是倍率按 1 美元 = 7 元折算（2026-09-24 使用者定的）：页面上只写「元」。
 */
const BANNED = [
  "中国",
  "大陆",
  "香港",
  "内地",
  "招商",
  "CN BIN",
  "HK BIN",
  "人民币",
  "WildCard",
  "Velo",
  "OCBC",
  "华侨",
  "华美",
];

// jsdom 没有实现 <dialog> 的 showModal / close，弹窗组件靠它们开关。
beforeEach(() => {
  const proto = HTMLDialogElement.prototype as HTMLDialogElement & {
    showModal?: () => void;
    close?: () => void;
  };
  if (typeof proto.showModal !== "function")
    proto.showModal = function (this: HTMLDialogElement) {
      this.setAttribute("open", "");
    };
  if (typeof proto.close !== "function")
    proto.close = function (this: HTMLDialogElement) {
      this.removeAttribute("open");
    };
  localStorage.clear();
});
afterEach(cleanup);

function confirmAck() {
  fireEvent.click(screen.getByRole("button", { name: "我读完了，进入指南" }));
}

function clickTab(name: string) {
  fireEvent.click(screen.getByRole("tab", { name: new RegExp(`^${name}`) }));
}

describe("Subscription page", () => {
  it("lands on the homepage and keeps the how-to tabs locked until acknowledged", () => {
    render(<Subscription />);
    expect(screen.getByRole("heading", { level: 1 })).toHaveProperty(
      "textContent",
      "官方订阅指南",
    );
    // 首页不需要确认就能看：结论区与计算器都在。
    expect(screen.getByText("换算成中转站倍率")).toBeTruthy();
    expect(localStorage.getItem(ACK_KEY)).toBeNull();

    clickTab("苹果内购");
    expect(screen.getByText("先读一遍知情说明")).toBeTruthy();
    expect(screen.queryByText(/准备一个美区 Apple 账户/)).toBeNull();

    confirmAck();
    expect(localStorage.getItem(ACK_KEY)).toBe("true");
    expect(screen.getByText(/准备一个美区 Apple 账户/)).toBeTruthy();
  });

  it("remembers the acknowledgement and switches between all five tabs", () => {
    localStorage.setItem(ACK_KEY, "true");
    render(<Subscription />);
    expect(screen.queryByText("先读一遍知情说明")).toBeNull();

    clickTab("安卓");
    expect(screen.getByText(/千万不要买 Google Play 礼品卡/)).toBeTruthy();
    clickTab("网页绑卡");
    expect(screen.getByText(/网页绑卡：不需要苹果设备/)).toBeTruthy();
    clickTab("答疑与合规");
    expect(screen.getByText("常见问题")).toBeTruthy();
    // FAQ 折叠：点问题展开答案。
    fireEvent.click(screen.getByRole("button", { name: FAQ_LIST[0].q }));
    expect(screen.getByText(FAQ_LIST[0].a)).toBeTruthy();
    clickTab("首页");
    expect(screen.getByText("不正规中转站会遇到什么")).toBeTruthy();
  });

  it("computes the multiplier the way the user specified: spend ÷ monthly value", () => {
    render(<Subscription />);
    const amount = screen.getByLabelText("你为会员付了多少钱（元 / 月）");
    // 默认套餐是 Claude Max 20x：周额度中间值 $4,000 × 4 = $16,000。40 元 ÷ 16000 ≈ 0.003×
    fireEvent.change(amount, { target: { value: "40" } });
    expect(screen.getByText("0.003×")).toBeTruthy();
    expect(screen.getByText(/相当于中转站倍率 0\.003×/)).toBeTruthy();
    // 官方那一档按 1:7 折成元：$200 × 7 ÷ $16,000 = 0.0875 → 0.088×，跟价目表同一个数。
    expect(screen.getByText(/\$200 × 7 ÷ \$16,000 = 0\.088×/)).toBeTruthy();

    // 中转站给 $100 额度 → 40 ÷ 100 = 0.4×，是官方 0.0875× 的 4.6 倍。
    fireEvent.change(screen.getByLabelText("中转站给你的额度（美元，选填）"), {
      target: { value: "100" },
    });
    expect(screen.getByText("0.4×")).toBeTruthy();
    expect(screen.getByText("4.6 倍")).toBeTruthy();

    // 那条线路标 ×0.2：额度按分组倍率扣，实际倍率 0.4 × 0.2 = 0.08×
    // （「中转站百科」帖：「输入 5 元，给你倍率 0.1，实际就是 5 × 0.1 = 0.5 元」）。
    // 少乘这一项会把中转算贵五倍。
    fireEvent.change(
      screen.getByLabelText("那条线路的倍率（×，选填，默认 1）"),
      { target: { value: "0.2" } },
    );
    expect(screen.getByText("0.08×")).toBeTruthy();
    expect(screen.getByText(/那条线路 ×0\.2/)).toBeTruthy();
    expect(screen.getByText("0.9 倍")).toBeTruthy();

    // 换套餐会把分母换成那档的：ChatGPT Pro 20x 周 $2,500 × 4 = $10,000。
    fireEvent.change(screen.getByLabelText("按哪档官方套餐折算"), {
      target: { value: "chatgpt-pro-20" },
    });
    expect(
      (
        screen.getByLabelText(
          "一个月总共能用多少刀（API 等值，美元）",
        ) as HTMLInputElement
      ).value,
    ).toBe("10000");
    expect(screen.getByText("0.004×")).toBeTruthy();
  });

  // 使用者 2026-09-24 在旧版上算的那一笔：Claude Pro 折成元是 140，÷ $1,400 = 0.1×，
  // 而旧价目表写的是 $20 ÷ $1,400 = 0.014× —— 同一页两个数。现在两边都是 0.1×。
  it("agrees with the plan table for the same plan (Claude Pro, 140 元)", () => {
    render(<Subscription />);
    fireEvent.change(screen.getByLabelText("按哪档官方套餐折算"), {
      target: { value: "claude-pro" },
    });
    fireEvent.change(screen.getByLabelText("你为会员付了多少钱（元 / 月）"), {
      target: { value: "140" },
    });
    expect(screen.getByText(/相当于中转站倍率 0\.1×/)).toBeTruthy();
    expect(screen.getByText(/\$20 × 7 ÷ \$1,400 = 0\.1×/)).toBeTruthy();
    expect(screen.getByText("和自己订阅官方一个价位")).toBeTruthy();
    const row = screen
      .getByText("Claude Pro", {
        selector: ".qb-sub-plan-name",
      })
      .closest("tr")!;
    expect(row.textContent).toContain("0.1×");
  });

  it("labels the 1:7 conversion, says it leans expensive, and drops the old claims", () => {
    render(<Subscription />);
    expect(
      screen.getByRole("columnheader", { name: "等效倍率（1:7）" }),
    ).toBeTruthy();
    expect(screen.getByText(/实际汇率一般在 6\.8 左右/)).toBeTruthy();
    const text = document.body.textContent ?? "";
    expect(text).toContain("7 是往贵了取的");
    // 按美元除美元的旧数（Max 20x 0.013×）与据此说的「比任何中转站都便宜」不许回来。
    expect(text).not.toContain("0.013×");
    expect(text).not.toContain("比任何中转站都便宜");
    expect(text).not.toContain("比最便宜的逆向中转还低");
  });

  it("never shows NaN or Infinity for bad input", () => {
    render(<Subscription />);
    const amount = screen.getByLabelText("你为会员付了多少钱（元 / 月）");
    fireEvent.change(amount, { target: { value: "abc" } });
    expect(screen.getByText("请填一个正数")).toBeTruthy();
    fireEvent.change(amount, { target: { value: "40" } });
    fireEvent.change(
      screen.getByLabelText("一个月总共能用多少刀（API 等值，美元）"),
      { target: { value: "0" } },
    );
    expect(document.body.textContent).not.toMatch(/NaN|Infinity/);
    expect(screen.getByText("填上金额，这里马上算出倍率。")).toBeTruthy();
  });

  it("keeps the data internally consistent", () => {
    // 每条坏处引用的来源都存在；每档套餐 iOS 价不低于网页价。
    const ids = new Set(SOURCES.map((s) => s.id));
    for (const d of RELAY_DOWNSIDES)
      for (const ref of d.sources) expect(ids.has(ref), ref).toBe(true);
    for (const p of PLANS)
      expect(p.iosMonthly).toBeGreaterThanOrEqual(p.webMonthly - 0.01);
    expect(new Set(SOURCES.map((s) => s.url)).size).toBe(SOURCES.length);
  });

  it("renders no banned words anywhere — modal, all five tabs, every FAQ answer", () => {
    render(<Subscription />);
    const seen: string[] = [];
    const collect = () => seen.push(document.body.textContent ?? "");
    collect(); // 弹窗 + 首页
    confirmAck();
    for (const tab of ["苹果内购", "安卓", "网页绑卡", "答疑与合规"]) {
      clickTab(tab);
      collect();
    }
    for (const item of FAQ_LIST) {
      fireEvent.click(screen.getByRole("button", { name: item.q }));
      collect();
    }
    const text = seen.join("\n");
    for (const word of BANNED) expect(text, word).not.toContain(word);
  });
});
