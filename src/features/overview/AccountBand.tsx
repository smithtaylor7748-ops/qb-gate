/**
 * 总览 Claude 页签：左边账户槽位，右边启动。
 *
 * # v0.7.0 搬走了两样东西
 *
 * * **Codex 磁贴**去了 `GptBand.tsx`。它跟左边的账户槽位没有任何关系 ——
 *   槽位换的是 `claude-profile` 目录联结点，Codex 的凭证在 `~/.codex`，
 *   完全是另一套。可它原来长得和另外三格一模一样。
 * * **一键关闭**去了 `KillBar.tsx` 并挪到页签外通栏。它按双重证据收进程，
 *   两侧的都收；留在 Claude 页签里会让人以为它只关 Claude。
 *
 * # 为什么账户与启动这两件事合在一段里
 *
 * 因为它们是同一个动作的两半：**切换账户之后总要再启动一次**
 * ——「正在跑的会话不会自动换过去」这句话确认框里一直写着。
 * 分成两张卡的时候，用户得先在上面切一次、再滚到下面点一次启动。
 *
 * # 槽位一页 5 条
 *
 * 6 个槽位竖排会比右边的启动宫格高出一截。分页之后左栏固定 5 行 + 一行页码，
 * 跟右栏的 2×2 宫格 + 一键关闭条高度基本齐平。
 *
 * # 「切换」这个词对过期槽位是错的
 *
 * `logged_in` 只是「`.credentials.json` 在不在」（`accounts/mod.rs`），
 * `cli_days_left` 是 refreshToken 剩余天数，两者故意不合并 ——
 * **过期槽位必须仍然可切**，因为你得先切过去才能在那个槽里重新登录（档案 §4.8，
 * `expired_slot_is_still_switchable` 那个单测钉着）。
 * 所以这里不禁用按钮，只换文案：过期的写「切换并重登」，没登过的写「切换并登录」。
 *
 * # v0.8.0：切换对话框不在这里了
 *
 * 搬去了 `pages/accounts/AccountDialogs.tsx`，全局只挂一份 —— 原来这里和账户页
 * 各挂一份，已经各走各的了。那边文件头写着 v0.9.0 的切换语义：
 * 先关闭全部 Claude，再切，**不自动启动任何东西** —— 用户切完回这里自己点启动磁贴。
 */

import { useState } from "react";
import {
  MonitorSmartphone,
  PlayCircle,
  Plus,
  Terminal,
  Users,
  Wine,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";

import { api, type LaunchTarget, type Slot } from "../../lib/api";
import { Link } from "react-router-dom";
import { workspaceApi, type Client } from "../../lib/workspace";
import { AFTER, R } from "../../lib/resources";
import { invalidate, useResource, useSession } from "../../lib/store";
import { endTask, resetTask, useTask } from "../../lib/tasks";
import {
  Button,
  Card,
  ConfirmDialog,
  Pill,
  useToast,
  fmtDaysLeft,
} from "../../ui";

import {
  needsLogin,
  requestNewSlot,
  requestSwitch,
} from "../../pages/accounts/AccountDialogs";
import Tile from "./Tile";
import SlotUsageBars from "./SlotUsage";

/** 一页几条。改这个数要顺带看一眼右栏高度还齐不齐。 */
const PER_PAGE = 5;

const CLAUDE_USAGE_URL = "https://claude.ai/settings/usage";

/** 启动目标 → 任务名。写成三元表达式的话，加第三个目标必然漏改。 */
const LAUNCH_TASK = {
  "claude-code": "launch-claude-code",
  "claude-desktop": "launch-claude-desktop",
  codex: "launch-codex",
} as const;

function switchLabel(s: Slot): string {
  if (!s.logged_in) return "切换并登录";
  if ((s.cli_days_left ?? 0) < 0) return "切换并重登";
  return "切换";
}

/** 剩余天数的语气。`null` 是「读不出到期时间」，不是「过期」，所以给中性。 */
function daysTone(s: Slot) {
  if (!s.logged_in) return "default" as const;
  const d = s.cli_days_left;
  if (d == null) return "default" as const;
  if (d < 0) return "danger" as const;
  if (d < 5) return "warn" as const;
  return "ok" as const;
}

// ---------------------------------------------------------------- 主体

export default function AccountBand() {
  const toast = useToast();

  const accounts = useResource("accounts", R.accounts);
  const plugins = useResource("plugins", R.plugins);

  const codeTask = useTask("launch-claude-code");
  const desktopTask = useTask("launch-claude-desktop");
  const tavernTask = useTask("tavern-start");

  const [busy, setBusy] = useState("");
  const [onlyUsable, setOnlyUsable] = useSession("home.accounts.usable", false);
  /** `-1` = 这个会话里还没手动翻过页，落在激活槽位那一页。 */
  const [pageRaw, setPage] = useSession("home.accounts.page", -1);
  const [askDesktop, setAskDesktop] = useState(false);

  const slots = accounts.data?.slots ?? [];
  const usableSlots = slots.filter(
    (s) => s.logged_in && (s.cli_days_left ?? 0) >= 0,
  );
  const shown = onlyUsable ? usableSlots : slots;

  const pages = Math.max(1, Math.ceil(shown.length / PER_PAGE));
  // 没手动翻过页时落在激活槽位那一页 —— 否则开面板第一眼看不到自己在用哪个。
  const autoPage = Math.floor(
    Math.max(
      0,
      shown.findIndex((s) => s.active),
    ) / PER_PAGE,
  );
  const page = Math.min(pageRaw < 0 ? autoPage : pageRaw, pages - 1);
  const pageSlots = shown.slice(page * PER_PAGE, page * PER_PAGE + PER_PAGE);

  const running = plugins.data?.[0]?.state === "running";

  // ------------------------------------------------------------ 动作

  async function launch(target: LaunchTarget) {
    const task = LAUNCH_TASK[target];
    setBusy(target);
    resetTask(task);
    try {
      // **必须**走 workspaceApi.launch，不是 api.launchClaude。
      // 后者拿不到身份参数（起不了指定槽位），而且无条件挂看门狗；
      // 前者才是 `if target.gated()` —— 中转会话不归门禁的关停策略管。
      const active = slots.find((s) => s.active);
      await workspaceApi.launch(
        target as Client,
        "official",
        target === "codex" ? "" : (active?.label ?? ""),
      );
      endTask(task);
      toast.ok("官方会话已启动");
      invalidate(...AFTER.lease);
    } catch (e) {
      // 门禁不过就一个进程都不起 —— 错误原文已经说清是 IP 不在白名单还是
      // 根本查不到 IP，两者必须分开，不能合并成「启动失败」。
      const msg = e instanceof Error ? e.message : String(e);
      endTask(task, msg);
      toast.error(msg);
    } finally {
      setBusy("");
      setAskDesktop(false);
    }
  }

  async function launchTavern() {
    setBusy("tavern");
    resetTask("tavern-start");
    try {
      const url = await api.pluginStart();
      endTask("tavern-start");
      // 无论是新起的还是复用现有服务都要打开页面 ——
      // 少了这一步，成功的启动和崩溃看起来一模一样。
      if (url.startsWith("http")) await openUrl(url);
      toast.ok("酒馆已就绪，已打开页面");
      invalidate(...AFTER.tavern);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("tavern-start", msg);
      toast.error(msg);
    } finally {
      setBusy("");
    }
  }

  return (
    <>
      <Card className="mb-3">
        <div className="grid gap-4 md:grid-cols-[1.3fr_1fr]">
          {/* ------------------------------------------------ 左：槽位 */}
          <div className="flex min-w-0 flex-col gap-1.5">
            <div className="mb-1 flex items-center gap-2">
              <h2 className="card-title">
                <Users size={14} aria-hidden="true" />
                账户槽位
              </h2>
              <span className="ml-auto flex flex-shrink-0 gap-1.5">
                <Button
                  size="sm"
                  variant={onlyUsable ? "primary" : "default"}
                  onClick={() => {
                    setOnlyUsable(!onlyUsable);
                    setPage(0);
                  }}
                >
                  {onlyUsable
                    ? `可用 ${usableSlots.length}`
                    : `全部 ${slots.length}`}
                </Button>
                <Button
                  size="sm"
                  onClick={() => void openUrl(CLAUDE_USAGE_URL)}
                >
                  Usage
                </Button>
                <Button
                  size="sm"
                  icon={<Plus size={12} />}
                  onClick={requestNewSlot}
                >
                  新建
                </Button>
                <Link className="btn btn--sm" to="/accounts">
                  管理
                </Link>
              </span>
            </div>

            {shown.length === 0 ? (
              <p className="notice">
                {slots.length === 0 ? (
                  // 原来这里写「用右边的 Claude Code 登录一次，第一个槽位就建好了」——
                  // 那不成立：没有槽位时 Claude Code 登录进的是它自己的默认目录，
                  // 永远不会凭空长出一个槽位来。
                  <>
                    还没有账户槽位。点上面的「新建」起个名字，再在里面登录一次。
                    没有槽位时，Claude Code 用的是它自己的默认目录{" "}
                    <code>~\.claude</code>。
                  </>
                ) : (
                  "当前筛选下没有槽位。点上面的「可用」切回全部。"
                )}
              </p>
            ) : (
              <>
                {pageSlots.map((s) => (
                  <div
                    key={s.label}
                    className={`slotrow${s.active ? " slotrow--active" : ""}`}
                  >
                    <div className="slotrow-main">
                      <span className="slotrow-label">{s.label}</span>
                      <span
                        className="slotrow-plan"
                        title={s.billing ?? undefined}
                      >
                        {s.plan ?? "套餐未知"}
                      </span>
                      <span className="slotrow-side">
                        <Pill tone={daysTone(s)}>
                          {s.logged_in
                            ? fmtDaysLeft(s.cli_days_left)
                            : "未登录"}
                        </Pill>
                        {s.active ? (
                          // 激活槽位过期时原来什么按钮都没有 —— 用户看到「凭证已过期」
                          // 却无处下手。这里补一个只启动、不切换的入口。
                          needsLogin(s) ? (
                            <Button
                              size="sm"
                              variant="primary"
                              loading={busy === "claude-code"}
                              disabled={!!busy}
                              onClick={() => launch("claude-code")}
                            >
                              重新登录
                            </Button>
                          ) : (
                            <span className="text-xs text-accent">使用中</span>
                          )
                        ) : (
                          <Button
                            size="sm"
                            disabled={!!busy}
                            onClick={() => requestSwitch(s.label)}
                          >
                            {switchLabel(s)}
                          </Button>
                        )}
                      </span>
                    </div>
                    {/* 额度。两源都没有就整条不画 —— 给一个「剩 100%」
                        比不给更糟，那是替一个根本没读到的数字打包票。 */}
                    {s.usage && <SlotUsageBars usage={s.usage} />}
                  </div>
                ))}

                <div className="mt-1 flex items-center gap-2">
                  <span className="notice">
                    {shown.length} 个槽位
                    {pages > 1 && ` · 第 ${page + 1} / ${pages} 页`}
                  </span>
                  {pages > 1 && (
                    <span className="pager ml-auto">
                      {Array.from({ length: pages }, (_, i) => (
                        <button
                          key={i}
                          type="button"
                          className="pager-btn"
                          aria-current={i === page}
                          aria-label={`第 ${i + 1} 页`}
                          onClick={() => setPage(i)}
                        >
                          {i + 1}
                        </button>
                      ))}
                    </span>
                  )}
                </div>

                <p className="notice mt-1">{accounts.data?.planCaveat}</p>
                <p className="notice">{accounts.data?.caveat}</p>
              </>
            )}
          </div>

          {/* ------------------------------------------------ 右：启动 */}
          <div className="flex min-w-0 flex-col gap-2 md:border-l md:border-line md:pl-4">
            <div className="mb-1 flex items-center gap-2">
              <h2 className="card-title">
                <PlayCircle size={14} aria-hidden="true" />
                启动
              </h2>
              <span className="notice ml-auto">门禁不过，一个进程都不起</span>
            </div>

            <div className="tilegrid">
              <Tile
                icon={<Terminal size={18} />}
                name="Claude Code"
                note="15 秒一次 · 断网先上锁留进程"
                tone="accent"
                task={codeTask}
                disabled={!!busy}
                onClick={() => launch("claude-code")}
              />
              {/* 这段代价必须常驻在贴上，不能藏进 hover —— 它不是 bug 是设计，
                  但用户有权在点之前就知道。完整版在下面那个强制确认框里。 */}
              <Tile
                icon={<MonitorSmartphone size={18} />}
                name="Claude 桌面端"
                note="查不到 IP 立即关闭，不给宽限"
                tone="warn"
                task={desktopTask}
                disabled={!!busy}
                onClick={() => setAskDesktop(true)}
              />
              <Tile
                icon={<Wine size={18} />}
                name="酒馆"
                note={
                  running
                    ? "运行中 · 再点只打开页面"
                    : "起桥接与酒馆 · 最长 80 秒"
                }
                task={tavernTask}
                disabled={!!busy}
                onClick={launchTavern}
              />
            </div>
          </div>
        </div>
      </Card>

      {/* --------------------------------------------------- 确认框 */}

      <ConfirmDialog
        open={askDesktop}
        onCancel={() => setAskDesktop(false)}
        onConfirm={() => launch("claude-desktop")}
        title="启动 Claude 桌面端？"
        confirmLabel="我知道了，启动"
        loading={busy === "claude-desktop"}
        danger
      >
        <p>
          桌面端这一档的看门狗<strong>不给宽限</strong>：每 20 秒查一次出口 IP，
          一旦查不到就立即关闭桌面端，不等网络恢复。
        </p>
        <p className="mt-2">
          这不是缺陷，是刻意的 —— 桌面端冻不住（真正在跑的那份不能加执行锁），
          「等等看」的实际含义就是让它在无法核实的网络上继续跑。
        </p>
        <p className="mt-2">
          代价：VPN 重连或 IP 查询服务限流会直接关掉你正在用的窗口，
          <strong>没保存的对话会丢</strong>。
        </p>
      </ConfirmDialog>
    </>
  );
}
