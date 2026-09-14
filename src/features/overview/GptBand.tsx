/**
 * 总览的 GPT 侧。
 *
 * # 为什么要跟 Claude 分开
 *
 * 原来的 2×2 启动宫格里，Claude Code、Claude 桌面端、酒馆、Codex 挤在一起，
 * 而左边的账户槽位**只对 Claude 有效**。于是「槽位切换 + 启动」这个组合
 * 对 Codex 那一格是无意义的，可它长得和另外三格一模一样。
 *
 * 分开之后每一侧只讲自己的事：Claude 侧是「用哪个账户、起哪个客户端」，
 * GPT 侧是「Codex 装了吗、归不归门禁管、走哪个中转站」。
 *
 * # 这一侧没有账户槽位，是对的
 *
 * 账户槽位换的是 `claude-profile` 目录联结点，Codex 的凭证在 `~/.codex`，
 * 完全是另一套。**不要在这里放一个长得像槽位的东西** —— 那会让人以为
 * 切换会连 Codex 一起切。Codex 侧真正的「切换」是中转站，所以这里给的是
 * 当前中转站与一个跳转入口。
 */

import { Settings2, SquareTerminal } from "lucide-react";

import { type LaunchTarget } from "../../lib/api";
import { Link } from "react-router-dom";
import { useWorkspace, workspaceApi, type Client } from "../../lib/workspace";
import { AFTER, R } from "../../lib/resources";
import { invalidate, useResource } from "../../lib/store";
import { endTask, resetTask, useTask } from "../../lib/tasks";
import {
  Button,
  Card,
  ExternalLink,
  Metric,
  Pill,
  Row,
  useToast,
} from "../../ui";
import { openSecurity } from "../security/SecuritySheet";

import Tile from "./Tile";

const CODEX_HOME = "https://github.com/openai/codex";

export default function GptBand() {
  const toast = useToast();

  const sw = useResource("software", R.software);
  const settings = useResource("settings", R.settings);
  const workspace = useWorkspace();
  const codexTask = useTask("launch-codex");

  const gated = settings.data?.codex_under_gate ?? false;
  const codex = sw.data?.codex;

  /**
   * Codex 那边正在跑的会话用的是谁。
   *
   * 旧实现读的是 `relay.json` 里的 `active` 标记 —— 那个文件自迁移之后
   * **再没人写过**（`relay::store::save()` 全项目零调用者），显示的永远是
   * 迁移那一刻的旧值，用户在中转站页改完也不会变。
   *
   * 新系统没有「全局激活」这个概念：每次启动都显式挑一个使用环境。所以唯一
   * 还有「当前」语义的东西，就是**正在跑的那个会话的身份**。没有会话在跑就
   * 如实说没在跑，不假装有一个当前值。
   */
  const codexSession = workspace.data?.sessions.find(
    (s) => s.context.client === "codex" && s.state === "running",
  );
  const codexEnvironment = workspace.data?.environments.find(
    (e) => e.id === codexSession?.context.identity_id,
  );
  const activeRelay =
    codexSession?.context.identity_kind === "relay"
      ? workspace.data?.providers.find(
          (p) => p.id === codexEnvironment?.provider_id,
        )
      : undefined;

  async function launchCodex() {
    const target: LaunchTarget = "codex";
    resetTask("launch-codex");
    try {
      await workspaceApi.launch(target as Client, "official", "");
      endTask("launch-codex");
      toast.ok("Codex 会话已启动");
      invalidate(...AFTER.lease);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask("launch-codex", msg);
      toast.error(msg);
    }
  }

  return (
    <Card
      title="GPT · OpenAI"
      className="mb-3"
      actions={
        <span className="notice">
          {gated ? "归 IP 门禁管" : "不归 IP 门禁管"}
        </span>
      }
    >
      <div className="grid gap-4 md:grid-cols-2">
        {/* ------------------------------------------------ 左：状态 */}
        <div className="min-w-0">
          <div className="grid gap-2 sm:grid-cols-2">
            <Metric label="Codex CLI" loading={sw.loading && !sw.data}>
              {codex?.installed ? (
                <>
                  {codex.version ?? "已安装"}
                  <Pill tone="ok">已装</Pill>
                </>
              ) : (
                <Pill tone="default">未安装</Pill>
              )}
            </Metric>
            <Metric
              label="中转站"
              loading={workspace.loading && !workspace.data}
            >
              {!codexSession
                ? "未在运行"
                : activeRelay
                  ? activeRelay.name
                  : "官方直连"}
            </Metric>
          </div>

          <Row
            className="mt-2"
            side={
              <Button
                size="sm"
                icon={<Settings2 size={13} />}
                onClick={() => openSecurity("lock")}
              >
                门禁范围
              </Button>
            }
          >
            <span>
              {gated ? "Codex 已纳入 IP 门禁" : "Codex 当前不归 IP 门禁管"}
            </span>
            <span className="notice">
              {gated
                ? "出口 IP 不在白名单时，codex 会被系统拒绝执行 —— 跟 claude.exe 一样。"
                : "默认如此。打开开关之后它才会跟 claude.exe 一样被加执行锁。"}
            </span>
          </Row>

          <Row
            className="mt-1"
            side={
              <Link className="btn btn--sm" to="/relays">
                中转站
              </Link>
            }
          >
            <span>Codex 的凭证与中转站配置</span>
            <span className="notice">
              写 <code>~/.codex/config.toml</code> 与 <code>auth.json</code>
              ，增量改、自动备份。
              <strong>它跟左边 Claude 的账户槽位是两套东西</strong>—— 槽位换的是{" "}
              <code>claude-profile</code> 联结点，不会动 Codex。
            </span>
          </Row>
        </div>

        {/* ------------------------------------------------ 右：启动 */}
        <div className="flex min-w-0 flex-col gap-2 md:border-l md:border-line md:pl-4">
          <div className="mb-1 flex items-center gap-2">
            <h2 className="card-title">
              <SquareTerminal size={14} aria-hidden="true" />
              启动
            </h2>
            <span className="notice ml-auto">
              {gated ? "门禁不过就不起" : "不验 IP，直接起"}
            </span>
          </div>

          <div className="tilegrid">
            <Tile
              icon={<SquareTerminal size={18} />}
              name="Codex"
              note={
                codex?.installed
                  ? gated
                    ? "已纳入 IP 门禁 · 15 秒一次"
                    : "不归 IP 门禁管"
                  : "本机没装，先去「环境与安装」"
              }
              task={codexTask}
              disabled={!codex?.installed}
              onClick={() => void launchCodex()}
            />
          </div>

          {!codex?.installed && (
            <Link className="btn btn--sm mt-1" to="/software">
              去安装 Codex
            </Link>
          )}

          <p className="notice mt-1">
            npm 全局装出来的是 <code>codex.cmd</code> 批处理，不是 exe。
            给批处理加执行锁能挡住 <code>codex</code> 这个命令本身，
            但挡不住有人直接去调它内部那个 node 脚本 —— 这一层解决不了，
            界面如实说明。
          </p>
          <p className="notice">
            <ExternalLink href={CODEX_HOME}>Codex 官方仓库</ExternalLink>
          </p>
        </div>
      </div>
    </Card>
  );
}
