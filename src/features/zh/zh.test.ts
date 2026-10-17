import { describe, expect, it } from "vitest";

import type { ClaudeZhLeftovers } from "../../lib/generated/ClaudeZhLeftovers";
import type { CodexLocaleHome } from "../../lib/generated/CodexLocaleHome";
import type { CodexLocaleOutcome } from "../../lib/generated/CodexLocaleOutcome";
import type { CodexLocaleStatus } from "../../lib/generated/CodexLocaleStatus";
import { claudeZhItems } from "./ClaudeZhCleanup";
import { gptZhHomeText, gptZhOn, gptZhResult } from "./GptZh";

const home = (
  current: string | null,
  more: Partial<CodexLocaleHome> = {},
): CodexLocaleHome => ({
  label: "槽位",
  config: "C:\\x\\config.toml",
  current,
  error: null,
  repair: null,
  adopted: null,
  is_default: false,
  ...more,
});

describe("GPT 汉化「开」的判定", () => {
  const status = (homes: CodexLocaleHome[]): CodexLocaleStatus => ({
    enabled: true,
    homes,
    ours_running: 0,
    foreign_running: 0,
    processes_error: null,
    egress_running: false,
  });

  it("每一份都是 zh-CN 才算开；一份都没有不算", () => {
    expect(gptZhOn(null)).toBe(false);
    expect(gptZhOn(status([]))).toBe(false);
    expect(gptZhOn(status([home("zh-CN"), home("zh-CN")]))).toBe(true);
    // 开始菜单那份开着时默认那一份会被跳过 —— 那时按钮不许说「开」。
    expect(gptZhOn(status([home("zh-CN"), home(null)]))).toBe(false);
    expect(gptZhOn(status([home("ja-JP")]))).toBe(false);
    // GPT 自己写坏了的那份：它读不到里面的值，后端给的 current 是 null —— 不算开。
    expect(
      gptZhOn(status([home("zh-CN"), home(null, { repair: "[desktop]" })])),
    ).toBe(false);
  });

  it("每一份的那句话：读不了、能修、设了、没设各说各的", () => {
    expect(gptZhHomeText(home(null, { error: "坏了" }))).toContain(
      "没法自动修",
    );
    expect(gptZhHomeText(home(null, { repair: "[desktop]" }))).toContain(
      "[desktop] 重复定义了",
    );
    expect(gptZhHomeText(home("zh-CN"))).toBe("界面语言 zh-CN");
    expect(gptZhHomeText(home(null))).toContain("没设");
  });
});

describe("GPT 汉化点完之后那一句", () => {
  const outcome = (o: Partial<CodexLocaleOutcome>): CodexLocaleOutcome => ({
    changed: 0,
    already: 0,
    repaired: 0,
    skipped: 0,
    default_busy: false,
    lines: [],
    reopened: null,
    ...o,
  });

  it("按后端给的数说，不猜那几句话", () => {
    expect(gptZhResult(outcome({ changed: 3 }), "on")).toEqual({
      ok: true,
      text: "已设为中文（改了 3 份）。下次打开 GPT 生效。",
    });
    expect(
      gptZhResult(outcome({ changed: 3, repaired: 1 }), "on").text,
    ).toContain("其中 1 份顺手修了 GPT 自己写重复的表");
    // 一份都没改成、却一句错都没有 —— 09-26 之前就是这样弹「成功」的。
    expect(gptZhResult(outcome({ skipped: 2 }), "on").ok).toBe(false);
    expect(gptZhResult(outcome({ changed: 3, skipped: 1 }), "on").ok).toBe(
      false,
    );
    // 只有默认那份被别处起的 GPT 占着：说清楚该做什么。
    const busy = gptZhResult(
      outcome({ changed: 3, skipped: 1, default_busy: true }),
      "on",
    );
    expect(busy.ok).toBe(false);
    expect(busy.text).toContain("完全退出它");
    expect(gptZhResult(outcome({ already: 4 }), "on").text).toBe(
      "本来就是中文，没有要改的。",
    );
    expect(gptZhResult(outcome({ changed: 2 }), "off").text).toBe(
      "已恢复默认（改了 2 份）。",
    );
  });
});

describe("Claude 汉化残留的单子", () => {
  const none: ClaudeZhLeftovers = {
    empty: true,
    apps: [],
    zh_profiles: [],
    stray_3p: null,
    panel_copy: null,
    state_file: null,
    needs_close: false,
    desktop_running: 0,
  };
  const app = {
    version: "2.9939.2",
    dir: "C:\\x\\app-2.9939.2",
    current: true,
    zh_files: 0,
    backup_mb: 107 as number | null,
    changed: 3,
    restorable: true,
  };

  it("每一样残留一句，说清楚清的时候做什么", () => {
    expect(claudeZhItems(none)).toEqual([]);
    const items = claudeZhItems({
      ...none,
      empty: false,
      apps: [app],
      stray_3p: "C:\\x\\Claude-3p",
      panel_copy: "C:\\x\\plugins\\claude-desktop-zh-cn",
      state_file: "C:\\x\\claude-zh\\state.json",
      zh_profiles: [{ dir: "C:\\x\\Claude-工作", locale: "zh-CN" }],
    });
    expect(items).toHaveLength(5);
    expect(items[0]).toContain("3 个前端文件跟原版不一样");
    expect(items[0]).toContain("107 MB");
    expect(items[1]).toContain("Claude-工作");
    expect(items[1]).toContain("en-US");
  });

  it("换不回原版的那两种，只说去官网重装", () => {
    expect(
      claudeZhItems({
        ...none,
        apps: [{ ...app, backup_mb: null, zh_files: 3 }],
      })[0],
    ).toContain("官网重新下载安装包");
    expect(
      claudeZhItems({ ...none, apps: [{ ...app, restorable: false }] })[0],
    ).toContain("面板不碰");
  });
});
