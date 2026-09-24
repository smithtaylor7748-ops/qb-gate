import { call } from "./ipc";
import { res } from "./store";
import type { CodexAccounts } from "./generated/CodexAccounts";
import type { CodexDesktop } from "./generated/CodexDesktop";
import type { CodexUsage } from "./generated/CodexUsage";
import type { CodexUsageSummary } from "./generated/CodexUsageSummary";
import type { CodexRateLimitScan } from "./generated/CodexRateLimitScan";

export const codexApi = {
  accounts: () => call<CodexAccounts>("codex_accounts"),
  desktop: () => call<CodexDesktop>("codex_desktop_status"),
  create: (label: string) => call<string>("codex_create", { label }),
  switch: (id: string) => call<void>("codex_switch", { id }),
  launch: (id: string) => call<void>("codex_launch", { id }),
  close: () => call<void>("codex_close"),
  // 更新后打包服务需管理员注册，否则启动一律「拒绝访问 (os error 5)」。会弹 UAC。
  repairRegistration: () => call<void>("codex_repair_registration"),
  archive: (id: string) => call<void>("codex_archive", { id }),
  /**
   * 装（或更新）Codex 桌面端（0.28.0）：winget 的 Store 源优先，没成走 FE3 直连下载 +
   * Add-AppxPackage。**会先关掉正在跑的桌面端**，界面上事前说明。
   * `local` 给一个 `.msix` 路径就只装那一份；`force` = 同版本也重装（修复注册）。
   * 进度走 `install-codex-desktop` 任务。
   */
  install: (local: string | null, force: boolean) =>
    call<string>("codex_desktop_install", { local, force }),
  /** Store 上现在是哪一版。只查元数据，不下载不安装。 */
  latest: () => call<string>("codex_desktop_latest"),
  usage: (id: string, days: number) =>
    call<CodexUsage>("codex_usage", { id, days }),
  /**
   * 用量明细页的 GPT 一侧：按天、按模型、按官方 API 价折算的美元（今天 / 7 天 / 30 天三档）。
   * **零网络**，读 Codex 写在槽位里的会话记录。`days` 只认 0 / 1 / 7 / 30。
   */
  usageSummary: (id: string, days: number) =>
    call<CodexUsageSummary>("codex_usage_summary", { id, days }),
  /**
   * 这个槽位的额度窗口（5 小时 / 7 天，0.32.0）。
   *
   * **零网络** —— 读的是 Codex 自己写在会话记录里的 `rate_limits`，
   * 跟 Claude 读 `plan-usage-history.json` 同一条口径。
   *
   * ⚠ 它是**上一次请求时的快照**，不是此刻；`found` 为 `null` 时界面说
   * 「还没有带额度信息的会话记录」，**不是 0%**。
   */
  rateLimits: (id: string) =>
    call<CodexRateLimitScan>("codex_rate_limits", { id }),
  /**
   * 官方额度接口（`wham/usage`），按槽位读取，不切换当前账户。
   *
   * ⛔ `refresh: false` 只读「最近一次」问到的，**绝不联网**（没问过就是 `null`）；
   * 只有使用者点了那一行的刷新图标才传 `true`（2026-09-23 使用者定的：只手动刷新）。
   */
  quota: (id: string, refresh: boolean) =>
    call<import("./generated/TavernGptQuota").TavernGptQuota | null>(
      "codex_quota",
      { id, refresh },
    ),
};
export const CODEX_R = {
  accounts: res(codexApi.accounts, { pollMs: 15_000 }),
  desktop: res(codexApi.desktop, { pollMs: 30_000 }),
};
