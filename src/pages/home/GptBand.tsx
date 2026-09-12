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

import { Settings2, SquareTerminal } from 'lucide-react';

import { api, type LaunchTarget } from '../../lib/api';
import { useNav } from '../../lib/nav';
import { AFTER, R } from '../../lib/resources';
import { invalidate, useResource } from '../../lib/store';
import { endTask, resetTask, useTask } from '../../lib/tasks';
import { Button, Card, ExternalLink, Metric, Pill, Row, useToast } from '../../ui';

import Tile from './Tile';

const CODEX_HOME = 'https://github.com/openai/codex';

export default function GptBand() {
  const { go } = useNav();
  const toast = useToast();

  const sw = useResource('software', R.software);
  const settings = useResource('settings', R.settings);
  const relay = useResource('relay', R.relay);
  const codexTask = useTask('launch-codex');

  const gated = settings.data?.codex_under_gate ?? false;
  const codex = sw.data?.codex;

  /**
   * Codex 那一侧当前激活的中转站。没配就是官方直连。
   *
   * `relayList()` 回的是**一个扁平数组**（三个 target 混在一起），
   * 所以要同时按 target 和 active 过滤 —— 只按 active 过会挑到 Claude 那条。
   */
  const activeRelay = relay.data?.find((p) => p.target === 'codex' && p.active);

  async function launchCodex() {
    const target: LaunchTarget = 'codex';
    resetTask('launch-codex');
    try {
      const r = await api.launchClaude(target);
      endTask('launch-codex');
      toast.ok(r.detail);
      invalidate(...AFTER.lease);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask('launch-codex', msg);
      toast.error(msg);
    }
  }

  return (
    <Card
      title="GPT · OpenAI"
      className="mb-3"
      actions={
        <span className="notice">
          {gated ? '归 IP 门禁管' : '不归 IP 门禁管'}
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
                  {codex.version ?? '已安装'}
                  <Pill tone="ok">已装</Pill>
                </>
              ) : (
                <Pill tone="default">未安装</Pill>
              )}
            </Metric>
            <Metric label="中转站" loading={relay.loading && !relay.data}>
              {activeRelay ? activeRelay.name : '官方直连'}
            </Metric>
          </div>

          <Row
            className="mt-2"
            side={
              <Button size="sm" icon={<Settings2 size={13} />} onClick={() => go('settings')}>
                门禁范围
              </Button>
            }
          >
            <span>{gated ? 'Codex 已纳入 IP 门禁' : 'Codex 当前不归 IP 门禁管'}</span>
            <span className="notice">
              {gated
                ? '出口 IP 不在白名单时，codex 会被系统拒绝执行 —— 跟 claude.exe 一样。'
                : '默认如此。打开开关之后它才会跟 claude.exe 一样被加执行锁。'}
            </span>
          </Row>

          <Row
            className="mt-1"
            side={
              <Button size="sm" onClick={() => go('relay')}>
                中转站
              </Button>
            }
          >
            <span>Codex 的凭证与中转站配置</span>
            <span className="notice">
              写 <code>~/.codex/config.toml</code> 与 <code>auth.json</code>，增量改、自动备份。
              <strong>它跟左边 Claude 的账户槽位是两套东西</strong>——
              槽位换的是 <code>claude-profile</code> 联结点，不会动 Codex。
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
              {gated ? '门禁不过就不起' : '不验 IP，直接起'}
            </span>
          </div>

          <div className="tilegrid">
            <Tile
              icon={<SquareTerminal size={18} />}
              name="Codex"
              note={
                codex?.installed
                  ? gated
                    ? '已纳入 IP 门禁 · 15 秒一次'
                    : '不归 IP 门禁管'
                  : '本机没装，先去「环境与安装」'
              }
              task={codexTask}
              disabled={!codex?.installed}
              onClick={() => void launchCodex()}
            />
          </div>

          {!codex?.installed && (
            <Button size="sm" className="mt-1" onClick={() => go('environment')}>
              去安装 Codex
            </Button>
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
