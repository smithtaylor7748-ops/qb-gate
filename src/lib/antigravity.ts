/**
 * 反重力（Google Antigravity）、反重力 IDE 槽位与 Gemini CLI 槽位的 IPC（0.26.0 / 0.30.0）。
 *
 * 形状跟 `codexAccounts.ts` 一样：命令一处、资源一处。反重力**没有中转路径**
 * （端点写死在客户端里）。槽位有两种：
 *
 * - **反重力 IDE 槽位**（0.30.0）：一个槽位一个 `--user-data-dir`，登录在 IDE 自己的窗口里做，
 *   面板只问它状态库里令牌那一行的长度。**Hub 没有槽位**（令牌在 Windows 凭据管理器）；
 * - **Gemini CLI 槽位**：酒馆的 Gemini 桥接用，`GEMINI_CLI_HOME`。
 *
 * 账户状态（邮箱 / 档位 / 各模型剩余额度）与用量平时读 **IDE / 语言服务器写在本机的文件**。
 * 联网额度（5 小时 / 每周四格、AI 积分）只在使用者点刷新图标时问一次（2026-09-23），
 * 见 `accountQuota` / `hubQuota`。
 */
import { call } from "./ipc";
import { res } from "./store";
import type { AntigravityProduct } from "./generated/AntigravityProduct";
import type { AntigravityStatus } from "./generated/AntigravityStatus";
import type { AntigravityUsage } from "./generated/AntigravityUsage";
import type { AntigravityOnlineQuota } from "./generated/AntigravityOnlineQuota";

export const antigravityApi = {
  status: () => call<AntigravityStatus>("antigravity_status"),
  /** 验 IP → 关掉正在跑的 → 起 → 挂看门狗。Hub 起来后按设置自动附加汉化引擎。 */
  launch: (product: AntigravityProduct) =>
    call<void>("antigravity_launch", { product }),
  close: (product: AntigravityProduct) =>
    call<number>("antigravity_close", { product }),
  /**
   * 这个产品此刻有几个进程在跑。**只看不动**，启动前拿它决定要不要弹确认框。
   *
   * 走的是启动链自己那套证据（按安装目录认），所以弹窗说的跟真的会发生的是同一回事。
   * ⛔ 出错时**不许当成 0** —— 调用方要照旧弹框，见 `antigravity_running` 的注释。
   */
  running: (product: AntigravityProduct) =>
    call<number>("antigravity_running", { product }),
  /** Hub 的「自动检查更新」。只并入一个键，重启 Hub 生效。 */
  autoUpdateSet: (enabled: boolean) =>
    call<void>("antigravity_auto_update_set", { enabled }),
  /**
   * 一键安装（0.29.0）。A 路 winget 官方包，没成从 Google 自己的下载域直下、
   * 核 Authenticode 主体含 Google 之后静默装。进度走 `install-antigravity`。
   *
   * ⚠ 后端会**先关掉正在跑的那一份**（Electron 单实例，开着装不上），
   * 所以调用方要先弹确认框把这个代价说清楚。
   */
  install: (
    product: AntigravityProduct,
    local: string | null,
    force: boolean,
  ) => call<string>("antigravity_install", { product, local, force }),
  /** 官网上现在是哪一版。只读下载页，不下载不安装。 */
  latest: (product: AntigravityProduct) =>
    call<string>("antigravity_latest", { product }),
  // 反重力账户槽位（0.32.0：一条 = 一个 Google 账户，底下两半）
  /** 新建一条账户：IDE 资料目录与 Gemini CLI home **两半一起建**。 */
  ideCreate: (label: string) =>
    call<string>("antigravity_ide_create", { label }),
  /**
   * 切换 = 换激活槽位，不关不起任何东西；下次从面板起 IDE 才用它。
   *
   * ⚙ 酒馆的 Gemini 桥接在跑时后端会拒绝（切一次连 CLI 那一半一起换）。
   */
  ideSelect: (id: string) => call<void>("antigravity_ide_select", { id }),
  ideArchive: (id: string) => call<void>("antigravity_ide_archive", { id }),
  /** 把 `from` 那条并进 `into`。**只改索引，不搬目录**。 */
  accountAttach: (into: string, from: string) =>
    call<void>("antigravity_account_attach", { into, from }),
  accountRename: (id: string, label: string) =>
    call<void>("antigravity_account_rename", { id, label }),
  /**
   * 本机用量：扫语言服务器的对话记录库。`days`：1 今天 / 7 / 30 / 0 全部。
   * 零网络；「缓存省下」按 Claude 用量卡同一份价目算。
   */
  usage: (days: number) =>
    call<AntigravityUsage>("antigravity_usage", { days }),
  /**
   * 一条账户槽位的联网额度：档位、AI 积分、Claude / Gemini × 5 小时 / 每周四格（2026-09-23）。
   *
   * ⛔ `refresh: false` 只读「最近一次」问到的，**绝不联网**（没问过就是 `null`）；
   * 只有使用者点了那一行的刷新图标才传 `true`。令牌过期时后端在内存里换新，不写回。
   * 报错时界面**照实显示原因**，永远不显示一个猜出来的数。
   */
  accountQuota: (id: string, refresh: boolean) =>
    call<AntigravityOnlineQuota | null>("antigravity_account_quota", {
      id,
      refresh,
    }),
  /** Hub 的联网额度，形状与规矩同 `accountQuota`。 */
  hubQuota: (refresh: boolean) =>
    call<AntigravityOnlineQuota | null>("antigravity_hub_quota", { refresh }),
  // 账户的 Gemini CLI 那一半（酒馆的 Gemini 桥接用）
  //
  // 0.32.0 起没有单独的 Gemini 槽位了：列表 / 新建 / 切换 / 移除全走上面
  // 那组 `ide*`。剩下这一条是真正跟 CLI 绑死的动作。
  /** 打开一个 Gemini CLI 登录窗口（`id` 是**账户槽位**的 id）。面板不碰凭据。 */
  geminiLogin: (id: string) => call<void>("gemini_login", { id }),
  /**
   * `npm install -g @google/gemini-cli`。**等它装完**，进度走 `install-gemini-cli`。
   *
   * 0.29.0 之前这里是「弹一个 cmd 窗口就返回」，而那个窗口里 npm 从来没跑起来过
   * （`/k` 被加了引号，cmd 认不出是开关）—— 使用者看到的是「弹出了命令窗口，
   * 什么都没装」。详见 `usecase::antigravity_ops::gemini_cli_install`。
   */
  geminiCliInstall: () => call<string>("gemini_cli_install"),
};

export const AG_R = {
  status: res(antigravityApi.status, { pollMs: 15_000 }),
};

export const PRODUCT_LABEL: Record<AntigravityProduct, string> = {
  hub: "反重力",
  ide: "反重力 IDE",
};
