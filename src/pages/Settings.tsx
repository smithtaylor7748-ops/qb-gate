import { useEffect, useState } from 'react';
import { Clock, FileText, Info, Lock, Scale, Undo2 } from 'lucide-react';

import { api, type Settings as AppSettings, type UpdateStatus } from '../lib/api';
import { useNav } from '../lib/nav';
import { AFTER, R } from '../lib/resources';
import { invalidate, useResource } from '../lib/store';
import { ALL_PROMPTS, type PromptDef } from '../prompts';
import {
  Bullet,
  Button,
  Card,
  CodeBlock,
  Collapsible,
  ConfirmDialog,
  ExternalLink,
  PageHeader,
  Row,
  Checkbox,
  useToast,
} from '../ui';

export default function Settings() {
  const { go } = useNav();
  const toast = useToast();
  const [open, setOpen] = useState<PromptDef | null>(null);
  const [busy, setBusy] = useState('');
  const [ask, setAsk] = useState<null | 'tz' | 'release' | 'codexGate'>(null);
  const settings = useResource('settings', R.settings);
  // 兜底文案。真正的版本号来自 Rust 的 env!("CARGO_PKG_VERSION")，
  // 这里**不再写死一个会过期的数字** —— 上一版就是因为它一直停在 0.2.0，
  // 而 Cargo.toml 早就往前走了，界面上根本看不出本机装的是新是旧。
  const appVersion = '读取中…';
  const [updates, setUpdates] = useState<UpdateStatus | null>(null);

  useEffect(() => {
    let live = true;
    void api.updateStatus().then((s) => live && setUpdates(s)).catch(() => undefined);
    return () => { live = false; };
  }, []);

  async function act(name: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(name);
    try {
      await fn();
      toast.ok(ok);
      invalidate(...AFTER.gate, 'tz');
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
      setAsk(null);
    }
  }

  async function setCodexGate(on: boolean) {
    const next: AppSettings = { ...(settings.data ?? { codex_under_gate: false }), codex_under_gate: on };
    setBusy('codexGate');
    try {
      await api.settingsSave(next);
      toast.ok(
        on
          ? 'Codex 已纳入门禁。出口 IP 不合规时 codex 会被系统拒绝执行。'
          : 'Codex 已移出门禁，它身上的执行锁已摘除。'
      );
      invalidate('settings', 'gate');
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
      setAsk(null);
    }
  }

  const codexGated = settings.data?.codex_under_gate ?? false;

  return (
    <>
      <PageHeader title="设置" sub="提示词、门禁范围、还原操作、合规边界与来源致谢。" />

      {/* ------------------------------------------------ 门禁范围 */}
      <Card title="门禁范围" icon={<Lock size={14} />} className="mb-3">
        <Row
          side={
            <Checkbox
              checked={codexGated}
              disabled={!!busy || settings.loading}
              onChange={(on) => (on ? setAsk('codexGate') : void setCodexGate(false))}
            >
              {codexGated ? '已接管' : '未接管'}
            </Checkbox>
          }
        >
          <span>让 IP 锁也接管 Codex</span>
          <span className="notice">
            默认关闭。打开后 <code>codex</code> 会跟 <code>claude.exe</code>{' '}
            一样被加执行锁 —— 出口 IP 不在白名单时<strong>命令会被系统拒绝执行</strong>。
          </span>
        </Row>

        <Collapsible className="mt-1" summary="打开之前先知道这几件事">
          <p className="notice">
            <strong>它会影响你日常用 Codex。</strong>
            上锁之后要跑 Codex，得先在总览点「启动 Codex」走面板的受控入口，
            或者确认出口 IP 在白名单里。这跟 Claude Code 现在的行为完全一样。
          </p>
          <p className="notice mt-2">
            <strong>npm 装的是 <code>codex.cmd</code> 批处理，不是 exe。</strong>
            给批处理加执行锁能挡住 <code>codex</code> 这个命令本身，
            但挡不住有人直接去调它内部那个 node 脚本 ——
            这跟桌面端 <code>app-*</code> 那个已知缺口是同一类，
            要彻底堵死需要 AppLocker / WDAC，不在本项目范围内。
          </p>
          <p className="notice mt-2">
            关掉这个开关时，面板会<strong>主动把 Codex 身上的锁摘掉</strong>。
            不摘的话它已经不在门禁清单里了，往后谁都不会再碰它，
            你会得到一个永远跑不起来的 codex。
          </p>
        </Collapsible>
      </Card>

      <Card title="ClaudeGate 版本" icon={<Info size={14} />} className="mb-3">
        <Row side={<span className="font-mono">v{updates?.current_version ?? appVersion}</span>}>
          <span>当前版本</span>
          <span className="notice">{updates?.detail ?? '更新状态读取中…'}</span>
        </Row>
        <p className="notice mt-2">启动时不会访问未配置的仓库，也不会下载或执行未经签名的更新包。仓库配置完成后将启用签名校验的一键更新。</p>
      </Card>

      {/* ---------------------------------------------------- 提示词 */}
      <Card title="内置提示词" icon={<FileText size={14} />} className="mb-3">
        <p className="notice mb-2">
          三份都刻意写成「先诊断、再报告、要确认才动手」——
          让模型直接对网络配置和软件安装动手，出错代价比多问一句大得多。
        </p>
        {ALL_PROMPTS.map((p) => (
          <Row
            key={p.id}
            side={
              <Button size="sm" onClick={() => setOpen(open?.id === p.id ? null : p)}>
                {open?.id === p.id ? '收起' : '查看'}
              </Button>
            }
          >
            <span>{p.title}</span>
            <span className="notice">{p.modelHint}</span>
          </Row>
        ))}
        {open && (
          <div className="mt-3">
            <CodeBlock text={open.body} caption={open.title} />
          </div>
        )}
      </Card>

      {/* ------------------------------------------------------ 还原 */}
      <Card title="还原" icon={<Undo2 size={14} />} className="mb-3">
        <Row
          side={
            <Button size="sm" icon={<Clock size={12} />} disabled={!!busy} onClick={() => setAsk('tz')}>
              还原
            </Button>
          }
        >
          <span>还原系统时区</span>
          <span className="notice">仅当本次会话切换过时区才有效</span>
        </Row>
        <Row
          side={
            <Button size="sm" variant="danger" disabled={!!busy} onClick={() => setAsk('release')}>
              执行
            </Button>
          }
        >
          <span>收回租约并重新上锁</span>
          <span className="notice">会锁住全部副本，不只是租出去的那一个</span>
        </Row>
      </Card>

      {/* -------------------------------------------------- 一键关闭 */}
      <Card title="一键关闭所有 Claude" tone="danger" className="mb-3">
        <p className="notice notice--danger">
          这个按钮在<strong>总览</strong>页。点之前会先列出会被收的进程让你确认。
        </p>
        <Collapsible className="mt-1" summary="它是怎么决定收哪些的？">
          <p className="notice">
            只收满足<strong>双重证据</strong>之一的进程：可执行文件由 Anthropic 签名，
            <strong>或者</strong>命令行同时命中 <code>bridge.py</code> 与本项目的数据目录。
          </p>
          <p className="notice mt-2">
            <strong>绝不按进程名杀</strong> —— 叫 claude.exe 或 python.exe 的东西
            可能是你正在干的别的活。放过的进程会连同理由一起显示。收完自动重新上锁。
          </p>
        </Collapsible>
        <Button className="mt-2" onClick={() => go('home')}>
          去总览
        </Button>
      </Card>

      {/* ---------------------------------------------------- 合规边界 */}
      {/* 这一段全项目**只出现在这一处**。旧版在账户页、中转站页、设置页
          各写了一遍，三处措辞还不完全一样。 */}
      <Card title="合规边界" icon={<Scale size={14} />} className="mb-3">
        <p className="notice mb-2">以下四条是本项目的硬约束，任何改动都不得破坏：</p>
        <Bullet marker="1.">不读取任何限流 / 429 / 额度状态，不存在「用完自动换号」的路径</Bullet>
        <Bullet marker="2.">账户切换只能由人手动触发，无定时器、无 watchdog、无自动调用点</Bullet>
        <Bullet marker="3.">任意时刻只有一个账户激活</Bullet>
        <Bullet marker="4.">所有账户必须是使用者本人拥有的</Bullet>
        <p className="notice mt-3">
          卸载清理功能的定位是：<strong>修复损坏安装、移交机器、清除本人数据</strong>。
          不为规避封禁或用量限制而设计 ——
          Anthropic 政策禁止为规避限制而创建或轮换多个账户。
        </p>
      </Card>

      {/* -------------------------------------------------- 来源致谢 */}
      <Card title="来源与致谢">
        <p className="notice mb-2">
          中文环境识别改编自 FuckClaude（MIT）；DNS 检测使用 bash.ws 的公开接口；
          中转站配置形态参考 cc-switch（MIT）。逐条说明见仓库根目录的 ATTRIBUTION.md。
        </p>
        <div className="flex flex-wrap gap-2">
          <ExternalLink href="https://github.com/LinXiaoTao/FuckClaude" asButton>
            FuckClaude
          </ExternalLink>
          <ExternalLink href="https://github.com/farion1231/cc-switch" asButton>
            cc-switch
          </ExternalLink>
          <ExternalLink href="https://ippure.com/" asButton>
            ippure.com
          </ExternalLink>
        </div>
      </Card>

      {/* ---------------------------------------------------- 确认框 */}

      <ConfirmDialog
        open={ask === 'codexGate'}
        onCancel={() => setAsk(null)}
        onConfirm={() => void setCodexGate(true)}
        title="让 IP 锁接管 Codex？"
        confirmLabel="确认接管"
        loading={busy === 'codexGate'}
      >
        <p>
          Codex 的可执行文件会被加上 Deny ExecuteFile。
          <strong>出口 IP 不在白名单时，`codex` 命令会被系统直接拒绝执行。</strong>
        </p>
        <p className="notice mt-2">
          之后要跑 Codex，请到总览点「启动 Codex」走受控入口。
          随时可以回到这里关掉，关掉时锁会自动摘除。
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={ask === 'tz'}
        onCancel={() => setAsk(null)}
        onConfirm={() => act('tz', api.tzRestore, '已还原为切换前的系统时区')}
        title="还原系统时区？"
        confirmLabel="确认还原"
        loading={busy === 'tz'}
      >
        <p>
          切回本次会话开始前的时区。<strong>需要管理员权限，会弹 UAC。</strong>
        </p>
        <p className="notice mt-2">
          如果本次会话没有切换过时区，这个操作什么都不会做。
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={ask === 'release'}
        onCancel={() => setAsk(null)}
        onConfirm={() => act('release', api.gateRelease, '租约已收回，全部副本重新上锁')}
        title="收回租约并重新上锁？"
        confirmLabel="确认执行"
        loading={busy === 'release'}
        danger
      >
        <p>
          全部 claude.exe 副本会重新加上 Deny ExecuteFile。
          <strong>正在跑的会话不会被杀掉</strong>，但它下次启动会被拒绝。
        </p>
      </ConfirmDialog>
    </>
  );
}
