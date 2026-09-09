import { useState } from 'react';
import {
  AlertTriangle,
  Lock,
  Plus,
  RotateCw,
  SkipForward,
  Trash2,
  Unlock,
} from 'lucide-react';

import { api, type GateStatus, type Risk } from '../lib/api';
import { useNav } from '../lib/nav';
import { AFTER, R } from '../lib/resources';
import { invalidate, peek, useResource } from '../lib/store';
import {
  Bullet,
  Button,
  Card,
  Collapsible,
  ConfirmDialog,
  EmptyState,
  LogView,
  Metric,
  PageHeader,
  Pill,
  Row,
  useToast,
  TARGET_KIND_HINT,
  TARGET_KIND_LABEL,
} from '../ui';

type Dialog = null | { kind: 'unlock' } | { kind: 'clean' } | { kind: 'remove'; ip: string };

export default function IpLock() {
  const { mark } = useNav();
  const toast = useToast();
  const gate = useResource('gate', R.gate);
  const st = gate.data;

  const [busy, setBusy] = useState('');
  const [dialog, setDialog] = useState<Dialog>(null);

  const emptyAllowlist = !!st && st.allowlist.length === 0;
  const locked = st?.targets.filter((t) => t.locked).length ?? 0;

  async function act(name: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(name);
    try {
      await fn();
      toast.ok(ok);
      invalidate(...AFTER.gate);
      await gate.refresh();
      await recordRisk();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
      setDialog(null);
    }
  }

  async function recordRisk() {
    const s = peek<GateStatus>('gate');
    if (!s) return;
    const risk: Risk =
      s.allowlist.length === 0 || !s.ip_allowed ? 'high' : s.stale_copies.length > 0 ? 'medium' : 'low';
    await mark(
      'iplock',
      risk === 'high' ? 'failed' : 'passed',
      risk,
      s.allowlist.length === 0
        ? '白名单为空，门禁不会放行任何 IP'
        : s.ip_allowed
          ? `当前 IP ${s.current_ip} 在白名单内`
          : `当前 IP ${s.current_ip ?? '未知'} 不在白名单内`
    );
  }

  return (
    <>
      <PageHeader
        title="IP 锁"
        sub="门禁的本体是 NTFS Deny ExecuteFile ACE —— 锁上之后连双击都会被系统拒绝。"
        actions={
          <Button icon={<RotateCw size={13} />} loading={gate.loading} onClick={() => void gate.refresh()}>
            刷新
          </Button>
        }
      />

      {gate.error && <p className="notice notice--danger mb-3">{gate.error}</p>}

      {/* 白名单为空时最要紧的一件事：告诉用户「面板没上锁」并给出一步修复。 */}
      {emptyAllowlist && (
        <Card tone="warn" className="mb-3">
          <div className="flex items-start gap-2">
            <AlertTriangle size={16} className="mt-0.5 flex-shrink-0" aria-hidden="true" />
            <div className="min-w-0">
              <div className="text-md text-[var(--warn)]">白名单是空的，门禁尚未启用</div>
              <p className="notice notice--warn mt-1">
                空名单意味着任何 IP 都过不了门禁，所以面板
                <strong>启动时不会上锁</strong> —— 否则你会被自己的工具关在门外。
                先把当前 IP 加进来。
              </p>
              <Button
                variant="primary"
                className="mt-2"
                icon={<Plus size={13} />}
                loading={busy === 'add'}
                disabled={!st?.current_ip}
                onClick={() =>
                  act('add', api.allowlistAddCurrent, `已把 ${st?.current_ip} 加进白名单`)
                }
              >
                把当前 IP 加进白名单
              </Button>
            </div>
          </div>
        </Card>
      )}

      <div className="mb-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
        <Metric
          label="当前出口"
          mono
          loading={gate.loading && !st}
          error={gate.error}
          onRetry={() => void gate.refresh()}
          emptyText="查不到"
          emptyHint="公网 IP 查询接口没返回。看门狗把「查不到」和「IP 变了」当两件事处理。"
        >
          {st?.current_ip}
        </Metric>
        <Metric label="是否在白名单" loading={gate.loading && !st} error={gate.error}>
          {st ? st.ip_allowed ? <Pill tone="ok">是</Pill> : <Pill tone="danger">否</Pill> : undefined}
        </Metric>
        <Metric label="执行锁" loading={gate.loading && !st} error={gate.error}>
          {st
            ? st.lease.holder
              ? `已租给 ${st.lease.holder}`
              : `${locked} / ${st.targets.length} 已锁`
            : undefined}
        </Metric>
        <Metric label="看门狗" loading={gate.loading && !st} error={gate.error}>
          {st ? (st.watchdog_running ? '运行中' : '未运行') : undefined}
        </Metric>
      </div>

      {/* ------------------------------------------------------ 白名单 */}
      <Card
        title="白名单"
        className="mb-3"
        actions={
          <Button
            size="sm"
            variant="primary"
            icon={<Plus size={12} />}
            loading={busy === 'add'}
            disabled={!st?.current_ip}
            onClick={() => act('add', api.allowlistAddCurrent, `已把 ${st?.current_ip} 加进白名单`)}
          >
            加入当前 IP
          </Button>
        }
      >
        {st?.allowlist.length ? (
          st.allowlist.map((ip) => (
            <Row
              key={ip}
              side={
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<Trash2 size={12} />}
                  disabled={!!busy}
                  onClick={() => setDialog({ kind: 'remove', ip })}
                >
                  删除
                </Button>
              }
            >
              <span className="font-mono">{ip}</span>
              {ip === st.current_ip && <Pill tone="ok">当前</Pill>}
            </Row>
          ))
        ) : (
          <EmptyState title="白名单为空">
            没有任何 IP 会被放行。面板因此不上锁 —— 这是刻意的，
            见上方提示。
          </EmptyState>
        )}
      </Card>

      {/* -------------------------------------------------- 受管文件 */}
      <Card title="受管可执行文件" className="mb-3">
        {st?.targets.length ? (
          st.targets.map((t) => (
            <Row
              key={t.path}
              side={
                <>
                  <Pill tone="default" title={TARGET_KIND_HINT[t.kind]}>
                    {TARGET_KIND_LABEL[t.kind] ?? t.kind}
                  </Pill>
                  {t.locked ? <Pill tone="ok">已锁</Pill> : <Pill tone="warn">未锁</Pill>}
                </>
              }
            >
              <span className="w-full break-all font-mono text-xs">{t.path}</span>
            </Row>
          ))
        ) : (
          <EmptyState title="没有找到可锁的 Claude 可执行文件">
            本机可能还没装 Claude，或者装在了面板不认识的位置。
          </EmptyState>
        )}

        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            icon={<Lock size={13} />}
            loading={busy === 'lock'}
            disabled={!!busy || emptyAllowlist}
            title={emptyAllowlist ? '白名单为空时不许上锁 —— 会把你自己关在门外' : undefined}
            onClick={() => act('lock', api.gateLockAll, '已重新上锁')}
          >
            立即全部上锁
          </Button>
          <Button
            variant="danger"
            icon={<Unlock size={13} />}
            disabled={!!busy}
            onClick={() => setDialog({ kind: 'unlock' })}
          >
            应急解锁
          </Button>
        </div>

        {/* 白名单为空时**直接拦住**上锁按钮，不能只靠 Rust 那边兜底。 */}
        {emptyAllowlist && (
          <p className="notice notice--warn mt-2">
            白名单为空，上锁按钮已停用 —— 锁上之后没有任何 IP 过得了门禁，
            解锁入口也会永远打不开。先加一条白名单。
          </p>
        )}

        <Collapsible className="mt-2" summary="为什么有一个不带锁的副本？">
          <p className="notice">
            <code>AnthropicClaude\app-&lt;版本&gt;\claude.exe</code> 不在上面的列表里。
            给它加 Deny ACE 会让桌面端开新窗口就崩，所以那一份只能靠看门狗
            taskkill 收。
          </p>
          <p className="notice mt-2">
            这是<strong>已知的残留缺口</strong>：刻意进那个目录双击能绕开启动门禁，
            20 秒内会被看门狗收掉 —— 前提是当时有看门狗在跑。
            要彻底堵死需要 AppLocker 或 WDAC，不在本项目范围内。
          </p>
        </Collapsible>

        <Collapsible summary="应急解锁为什么不验 IP？">
          <p className="notice">
            白名单为空、查不到公网 IP、IP 填错了 —— 这几种情况都会让正常入口
            永远过不了，那时 claude.exe 锁着而你打不开它。所以留一个不验 IP 的
            逃生口。
          </p>
          <p className="notice mt-2">
            安全上不吃亏：能点这个按钮的人本来就能改白名单文件、也能自己改 ACL。
            门禁防的是「跑起来之后出口 IP 悄悄变了」，
            <strong>不是防本机管理员</strong>。
          </p>
        </Collapsible>
      </Card>

      {/* -------------------------------------------------- 残留副本 */}
      {!!st?.stale_copies.length && (
        <Card title="升级残留副本" tone="danger" className="mb-3">
          <p className="notice notice--danger">
            这些副本<strong>没有 Deny ACL</strong>，是可以绕过门禁的执行副本。
            如果某一份正被运行中的会话占着会删不掉 —— 那是正常的，
            关掉对应进程后再清一次。
          </p>
          <div className="mt-2">
            {st.stale_copies.map((p) => (
              <Bullet key={p} tone="danger">
                <span className="break-all font-mono text-xs">{p}</span>
              </Bullet>
            ))}
          </div>
          <Button
            variant="danger"
            className="mt-3"
            icon={<Trash2 size={13} />}
            disabled={!!busy}
            onClick={() => setDialog({ kind: 'clean' })}
          >
            清理残留副本
          </Button>
        </Card>
      )}

      {/* ---------------------------------------------------- 看门狗 */}
      <Card title="看门狗" className="mb-3">
        {/* 这两条是说明，不是数据 —— 旧界面把它们做成了跟真实数据行一模一样的
            两栏行，看起来像是在报告当前配置。 */}
        <div className="rounded-[var(--radius-md)] bg-[var(--surface-2)] px-3 py-2">
          <Bullet marker="·">
            <strong>Claude Code / 桥接档</strong>：每 15 秒查一次。查不到 IP 先上锁
            保住进程，180 秒后才关停。
          </Bullet>
          <Bullet marker="·">
            <strong>Claude 桌面端档</strong>：每 20 秒查一次。查不到 IP
            <strong>立即关闭，不给宽限</strong>。
          </Bullet>
        </div>

        <Collapsible className="mt-2" summary="桌面端为什么不给宽限？">
          <p className="notice">
            它冻不住 —— 真正在跑的是 <code>app-*</code> 下那个不能加 Deny 的副本，
            而且早就把自己加载进内存了。对它来说「等等看」的实际含义就是
            「让它在无法核实的网络上继续跑」。
          </p>
          <p className="notice mt-2">
            代价：VPN 重连或 IP 查询服务限流会直接关掉正在用的桌面端，
            丢掉没保存的对话。这是刻意的取舍，不是缺陷。
          </p>
        </Collapsible>

        <div className="mt-3 flex flex-wrap gap-2">
          <Button
            variant="primary"
            loading={busy === 'wd-start'}
            disabled={!!busy || st?.watchdog_running}
            onClick={() =>
              act('wd-start', () => api.watchdogStart('Cli'), '看门狗已启动（CLI 档）')
            }
          >
            启动看门狗（CLI 档）
          </Button>
          <Button
            loading={busy === 'wd-stop'}
            disabled={!!busy || !st?.watchdog_running}
            onClick={() => act('wd-stop', api.watchdogStop, '看门狗已停止')}
          >
            停止看门狗
          </Button>
        </div>
      </Card>

      {/* ------------------------------------------------------ 日志 */}
      <Card title="ip-gate.log">
        <LogView lines={st?.recent_log ?? []} />
        <p className="notice mt-2">
          只记时间、公网 IP 与上锁解锁动作，<strong>不含任何对话内容</strong>。
        </p>
      </Card>

      <div className="mt-4 flex justify-end">
        <Button
          variant="ghost"
          icon={<SkipForward size={13} />}
          onClick={async () => {
            await mark('iplock', 'skipped', 'unknown', '用户强制跳过，未确认 IP 锁配置');
            toast.info('已标记为跳过');
          }}
        >
          标记为已跳过
        </Button>
      </div>

      {/* ---------------------------------------------------- 确认框 */}

      <ConfirmDialog
        open={dialog?.kind === 'unlock'}
        onCancel={() => setDialog(null)}
        onConfirm={() => act('unlock', api.gateUnlockAll, '已摘掉全部执行锁')}
        title="应急解锁？"
        confirmLabel="确认解锁"
        confirmWord="解锁"
        loading={busy === 'unlock'}
        danger
      >
        <p>
          这会<strong>摘掉全部 Deny ACE</strong>，而且不验证出口 IP。
          之后任何人都能直接双击 exe 把 Claude 跑起来，
          IP 门禁在你重新上锁之前形同虚设。
        </p>
        <p className="mt-2">
          只在被自己关在门外的时候用它：白名单为空、查不到公网 IP、或者 IP 填错了。
        </p>
        <p className="notice mt-2">
          解锁后记得处理完就回来点「立即全部上锁」。
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={dialog?.kind === 'clean'}
        onCancel={() => setDialog(null)}
        onConfirm={() => act('clean', api.gateCleanStale, '已清理残留副本')}
        title="删除这些残留副本？"
        confirmLabel="确认删除"
        loading={busy === 'clean'}
        danger
      >
        <p>下面这些文件会被<strong>永久删除</strong>：</p>
        <div className="mt-2">
          {st?.stale_copies.map((p) => (
            <Bullet key={p} tone="danger">
              <span className="break-all font-mono text-xs">{p}</span>
            </Bullet>
          ))}
        </div>
        <p className="notice mt-3">
          正被运行中的会话占着的那份会删不掉，那是正常的 —— Windows 允许改名
          正在运行的 exe，不允许删除它。关掉对应进程后再清一次即可。
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={dialog?.kind === 'remove'}
        onCancel={() => setDialog(null)}
        onConfirm={() =>
          dialog?.kind === 'remove' &&
          act(
            'remove',
            () => api.allowlistWrite((st?.allowlist ?? []).filter((x) => x !== dialog.ip)),
            `已从白名单删除 ${dialog.ip}`
          )
        }
        title="从白名单删除？"
        confirmLabel="确认删除"
        loading={busy === 'remove'}
        danger={dialog?.kind === 'remove' && dialog.ip === st?.current_ip}
      >
        <p>
          要删除的是 <code>{dialog?.kind === 'remove' ? dialog.ip : ''}</code>。
        </p>
        {dialog?.kind === 'remove' && dialog.ip === st?.current_ip && (
          <p className="notice notice--danger mt-2">
            <strong>这是你当前的出口 IP。</strong>
            删掉之后门禁不会再放行你，
            <strong>你会被自己的工具关在门外</strong>——
            那时只能走「应急解锁」。
          </p>
        )}
        {st?.allowlist.length === 1 && (
          <p className="notice notice--warn mt-2">
            删完白名单就空了。空名单时面板不会上锁，门禁等于关闭。
          </p>
        )}
      </ConfirmDialog>
    </>
  );
}
