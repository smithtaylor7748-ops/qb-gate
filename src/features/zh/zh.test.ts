import { describe, expect, it } from "vitest";

import type { ClaudeZhStatus } from "../../lib/generated/ClaudeZhStatus";
import type { CodexLocaleStatus } from "../../lib/generated/CodexLocaleStatus";
import { claudeZhLabel } from "./ClaudeZh";
import { gptZhOn } from "./GptZh";

const claude = (state: ClaudeZhStatus["state"]) =>
  ({ state }) as unknown as ClaudeZhStatus;

describe("Claude 汉化按钮的字", () => {
  it("跟着后端的状态走，读不到时不假装开着", () => {
    expect(claudeZhLabel(null).entry).toBe("汉化");
    expect(claudeZhLabel(undefined).text).toBe("读取中…");
    expect(claudeZhLabel(claude("on")).entry).toBe("汉化：开");
    expect(claudeZhLabel(claude("off")).entry).toBe("汉化");
    // Claude 自己更新之后汉化没了 —— 按钮上要直接说出来（使用者选的：点一下补上）。
    expect(claudeZhLabel(claude("needs-reapply")).entry).toBe(
      "汉化：需重新应用",
    );
    expect(claudeZhLabel(claude("partial")).tone).toBe("warn");
  });
});

describe("GPT 汉化「开」的判定", () => {
  const home = (current: string | null) => ({
    label: "槽位",
    config: "C:\\x\\config.toml",
    current,
    error: null,
    adopted: null,
    is_default: false,
  });
  const status = (homes: ReturnType<typeof home>[]): CodexLocaleStatus => ({
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
  });
});
