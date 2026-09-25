/** Claude official accounts: four slots per page; launch and current usage share the right column.
 * Switching stays manual and retains the existing account dialog and process checks.
 */

import { useState } from "react";
import {
  MonitorSmartphone,
  PlayCircle,
  Plus,
  RefreshCw,
  Terminal,
  Trash2,
  Users,
  Wine,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import BridgeSettings, { BridgeSettingsCorner } from "../tavern/BridgeSettings";
import { useNavigate } from "react-router-dom";

import { api, type LaunchTarget } from "../../lib/api";
import { workspaceApi, type Client } from "../../lib/workspace";
import { AFTER, R } from "../../lib/resources";
import { invalidate, useResource, useSession } from "../../lib/store";
import { endTask, resetTask, useTask } from "../../lib/tasks";
import { slotName } from "../../lib/slotName";
import { Button, Card, ConfirmDialog, Modal, useToast } from "../../ui";
import ClaudeZh, { claudeZhLabel } from "../zh/ClaudeZh";

import {
  needsLogin,
  requestAccountDetail,
  requestDelete,
  requestNewSlot,
  requestSwitch,
} from "../../pages/accounts/AccountDialogs";
import Tile from "./Tile";
import AccountUsageCard from "./AccountUsageCard";
import OfficialConfigResidue from "./OfficialConfigResidue";
import KillBar from "./KillBar";
import SlotRow, { switchLabel } from "./SlotRow";

/** Numbered pages keep launch controls beside the account list. */
const PER_PAGE = 4;

/** 启动目标 → 任务名。写成三元表达式的话，加第三个目标必然漏改。 */
const LAUNCH_TASK = {
  "claude-code": "launch-claude-code",
  "claude-desktop": "launch-claude-desktop",
  codex: "launch-codex",
  // 反重力两档不从这一栏起（有自己的 AntigravityBand），键只是让 Record 完整。
  antigravity: "launch-antigravity",
  "antigravity-ide": "launch-antigravity-ide",
} as const;

// ---------------------------------------------------------------- 主体

export default function AccountBand() {
  const toast = useToast();
  const navigate = useNavigate();
  const [bridgeOpen, setBridgeOpen] = useState(false);

  const accounts = useResource("accounts", R.accounts);
  const plugins = useResource("plugins", R.plugins);
  // 一键汉化（插件 claude-desktop-zh-cn，2026-09-25）。按钮文字如实显示现状，
  // 弹窗跟扩展中心的插件详情页是同一个组件。
  const zh = useResource("claudeZh", R.claudeZh);
  const [zhOpen, setZhOpen] = useState(false);

  const codeTask = useTask("launch-claude-code");
  const desktopTask = useTask("launch-claude-desktop");
  const tavernTask = useTask("tavern-start");

  const [busy, setBusy] = useState("");
  /** `-1` = 这个会话里还没手动翻过页，落在激活槽位那一页。 */
  const [pageRaw, setPage] = useSession("home.accounts.page", -1);
  const [askDesktop, setAskDesktop] = useState(false);

  const slots = accounts.data?.slots ?? [];
  // 0.20.0 去掉了「可用 / 全部」那个切换：过期和没登录的槽位**本来就该看得见**
  // （得先切过去才能在那个槽里重新登录），把它们藏起来只会让人找不到。
  const shown = slots;

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

  const tavern = plugins.data?.[0];
  const running = tavern?.state === "running";
  /**
   * 依赖还没配齐这一档（最常见的是三个路径一个都没填）。
   *
   * 这时候这块贴**点了只会失败** —— 而且失败信息里那句
   * 「路径不存在: bridge.py」曾经是使用者看到的全部内容。
   * 一个只能失败的按钮不该照常长着「起桥接与酒馆」的样子：
   * 改成在点之前就说明白，点下去带他到能填路径的那一页。
   */
  const tavernUnready = tavern?.state === "missing";

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
      invalidate(...AFTER.tavern);
      // 无论是新起的还是复用现有服务都要打开页面 ——
      // 少了这一步，成功的启动和崩溃看起来一模一样。反过来也一样：
      // 走到这里酒馆已经在跑、租约也拿着，页面没打开不许报成启动失败。
      try {
        if (url.startsWith("http")) await openUrl(url);
        toast.ok("酒馆已就绪，已打开页面");
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        toast.error(
          `酒馆已就绪，但页面没打开：${msg}。可以在浏览器里手动打开 ${url}`,
        );
      }
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
      <div className="account-workspace">
        <Card className="account-slots">
          {/* ------------------------------------------------ 左：槽位 */}
          <div className="account-slot-content">
            <div className="mb-1 flex items-center gap-2">
              <h2 className="card-title">
                <Users size={14} aria-hidden="true" />
                账户槽位
              </h2>
              <span className="ml-auto flex flex-shrink-0 gap-1.5">
                <Button
                  size="sm"
                  icon={<RefreshCw size={12} />}
                  loading={accounts.loading}
                  title="立刻重读一遍槽位与额度。后台每 30 秒自己也会读一次。"
                  aria-label="刷新账户"
                  onClick={() => void accounts.refresh()}
                />
                <Button
                  size="sm"
                  icon={<Plus size={12} />}
                  onClick={requestNewSlot}
                >
                  新建
                </Button>
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
                  "没有槽位。"
                )}
              </p>
            ) : (
              <>
                <div className="account-slot-list">
                  {pageSlots.map((s) => (
                    <SlotRow
                      key={s.label}
                      slot={s}
                      onOpen={requestAccountDetail}
                      // 额度条右边的刷新图标（2026-09-23）：Claude 只重读本机，不联网。
                      onRefresh={() => void accounts.refresh()}
                      refreshing={accounts.loading}
                      actions={
                        <>
                          {s.active ? (
                            // 激活槽位过期时原来什么按钮都没有 —— 用户看到
                            // 「凭证已过期」却无处下手。这里补一个只启动、
                            // 不切换的入口。没过期的那一档，重登入口在详情页里
                            // （令牌可能已被回收，而本地时间戳看不出来）。
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
                              <span className="text-xs text-accent">
                                使用中
                              </span>
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
                          {/* 「管理账户」就是删除，所以它直接长在条上，
                            不再跳去一个单独的管理页。当前账户删不了 ——
                            删掉它会留下一个指向空处的联结点，而没有任何
                            东西会去修它。 */}
                          <Button
                            size="sm"
                            variant="danger"
                            icon={<Trash2 size={12} />}
                            aria-label={`删除 ${slotName(s.email, s.label)}`}
                            disabled={!!busy || s.active}
                            title={
                              s.active
                                ? "当前账户删不了：请先切换到另一个账户。"
                                : `删除 ${slotName(s.email, s.label)}`
                            }
                            onClick={() => requestDelete(s.label)}
                          />
                        </>
                      }
                    />
                  ))}
                </div>
                <div className="account-pagination">
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

                {/* 口径说明（套餐读自哪儿、剩余天数查不出被风控下线）不在这里了
                    —— 使用者要这块地方放账户，说明搬进了账户详情页的
                    「套餐与凭证」一节，那里是真正要用到它的地方。
                    ⚠ 只是换了位置，**不是删掉**：`AccountsReport` 的类型注释
                    写着这两句要显示。 */}
              </>
            )}
          </div>
        </Card>
        <div className="account-workspace-right">
          <Card className="account-launch">
            {/* ------------------------------------------------ 右：启动 */}
            {/* 启动保持在右栏，用量紧接在其下方。 */}
            <div className="flex min-w-0 flex-col gap-2">
              <div className="mb-1 flex items-center gap-2">
                <h2 className="card-title">
                  <PlayCircle size={14} aria-hidden="true" />
                  启动
                </h2>
                {/* 一键汉化的入口放在标题右侧，不占纵向空间（照反重力页「汉化」的先例）。 */}
                <Button
                  size="sm"
                  variant={zh.data?.state === "on" ? "primary" : "default"}
                  className="turnstate-entry ml-auto"
                  data-testid="claude-zh-entry"
                  aria-label={`Claude 桌面端中文界面：${claudeZhLabel(zh.data).text}`}
                  onClick={() => setZhOpen(true)}
                >
                  {claudeZhLabel(zh.data).entry}
                </Button>
                {/* 原来标题右边常驻一句「门禁不过，一个进程都不起」，2026-09-23 使用者删了。
                    代价说明还在磁贴下面那一行（CLAUDE.md 要求代价常驻）。 */}
              </div>

              <div className="launchcol">
                <Tile
                  icon={<Terminal size={18} />}
                  name="Claude Code"
                  note="5 秒一次 · 查不到 IP 立即关闭，不给宽限"
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
                  note="5 秒一次 · 查不到 IP 立即关闭，不给宽限"
                  tone="warn"
                  task={desktopTask}
                  disabled={!!busy}
                  onClick={() => setAskDesktop(true)}
                />
                {/* 第四块：一键关闭。起和收是同一件事的两头，排在一起。
                  它收的不只是这一侧的东西 —— 名字里那个「所有」不许拿掉。 */}
                <Tile
                  icon={<Wine size={18} />}
                  name="酒馆"
                  note={
                    tavernUnready
                      ? "还没配好 · 点开去填路径"
                      : running
                        ? "运行中 · 再点只打开页面"
                        : "起桥接与酒馆 · 最长 80 秒"
                  }
                  tone={tavernUnready ? "warn" : undefined}
                  task={tavernTask}
                  disabled={!!busy}
                  onClick={
                    tavernUnready
                      ? () => navigate("/extensions/sillytavern")
                      : launchTavern
                  }
                  corner={
                    <BridgeSettingsCorner onClick={() => setBridgeOpen(true)} />
                  }
                />
                <KillBar variant="tile" />
              </div>

              <div className="mt-1">
                <OfficialConfigResidue
                  activeLabel={slots.find((s) => s.active)?.label ?? ""}
                />
              </div>
            </div>
          </Card>
          <AccountUsageCard />
        </div>
      </div>

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
          桌面端这一档的看门狗<strong>不给宽限</strong>：每 5 秒查一次出口 IP，
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
      {/* 0.32.0：三条桥的端口与模型 —— 三个账户页共用同一个组件。 */}
      <BridgeSettings
        open={bridgeOpen}
        provider="claude"
        onClose={() => setBridgeOpen(false)}
      />
      <Modal
        open={zhOpen}
        onClose={() => {
          setZhOpen(false);
          invalidate("claudeZh");
        }}
        title="Claude 桌面端 · 中文界面"
      >
        <ClaudeZh />
      </Modal>
    </>
  );
}
