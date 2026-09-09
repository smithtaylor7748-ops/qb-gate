import { useState } from 'react';
import {
  ArrowLeftRight,
  KeyRound,
  LockKeyhole,
  MonitorSmartphone,
  RotateCw,
  SkipForward,
  Terminal,
} from 'lucide-react';

import { api, type Risk } from '../lib/api';
import { useNav } from '../lib/nav';
import { AFTER, R } from '../lib/resources';
import { invalidate, useResource } from '../lib/store';
import {
  Button,
  Card,
  Collapsible,
  ConfirmDialog,
  EmptyState,
  Metric,
  PageHeader,
  Pill,
  Row,
  useToast,
  fmtDaysLeft,
} from '../ui';

type Dialog = null | { kind: 'switch'; label: string } | { kind: 'release' };

export default function Accounts() {
  const { mark, go } = useNav();
  const toast = useToast();

  const accounts = useResource('accounts', R.accounts);
  const gate = useResource('gate', R.gate);

  const [busy, setBusy] = useState('');
  const [dialog, setDialog] = useState<Dialog>(null);

  const slots = accounts.data?.slots ?? [];
  const active = slots.find((s) => s.active);
  const holder = gate.data?.lease.holder;

  async function act(name: string, fn: () => Promise<unknown>, ok: string) {
    setBusy(name);
    try {
      await fn();
      toast.ok(ok);
      invalidate(...AFTER.lease);
      await accounts.refresh();
      await recordRisk();
    } catch (e) {
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy('');
      setDialog(null);
    }
  }

  async function recordRisk() {
    if (!slots.length) return;
    const a = slots.find((s) => s.active);
    const risk: Risk = !a?.logged_in ? 'high' : (a.cli_days_left ?? 99) < 5 ? 'medium' : 'low';
    await mark(
      'accounts',
      risk === 'high' ? 'failed' : 'passed',
      risk,
      a ? `当前槽位 ${a.label}${a.logged_in ? '，已登录' : '，未登录'}` : '没有激活的槽位'
    );
  }

  return (
    <>
      <PageHeader
        title="账户与启动"
        sub="登录、切换槽位。启动入口在总览页。"
        actions={
          <Button
            icon={<RotateCw size={13} />}
            loading={accounts.loading}
            onClick={() => void accounts.refresh()}
          >
            刷新
          </Button>
        }
      />

      {accounts.error && <p className="notice notice--danger mb-3">{accounts.error}</p>}
      {accounts.data?.migration?.migrated?.length ? (
        <p className="notice notice--warn mb-3">
          已发现并迁移 {accounts.data.migration.migrated.length} 个旧配置。迁移前备份：{' '}
          <code>{accounts.data.migration.backup ?? '未创建'}</code>
        </p>
      ) : null}

      <div className="mb-3 grid gap-2 sm:grid-cols-3">
        <Metric
          label="当前槽位"
          loading={accounts.loading && !accounts.data}
          error={accounts.error}
          onRetry={() => void accounts.refresh()}
        >
          {active?.label}
        </Metric>
        <Metric label="登录态" loading={accounts.loading && !accounts.data}>
          {active ? (
            active.logged_in ? (
              <Pill tone="ok">已登录</Pill>
            ) : (
              <Pill tone="danger">未登录</Pill>
            )
          ) : undefined}
        </Metric>
        <Metric
          label="凭证剩余"
          loading={accounts.loading && !accounts.data}
          hint="只读本地时间戳，查不出被风控下线"
        >
          {active ? fmtDaysLeft(active.cli_days_left) : undefined}
        </Metric>
      </div>

      {/* ------------------------------------------------------ 槽位 */}
      <Card title="账户槽位" className="mb-3">
        {slots.length ? (
          slots.map((s) => (
            <Row
              key={s.label}
              side={
                <Button
                  size="sm"
                  icon={<ArrowLeftRight size={12} />}
                  disabled={!!busy || s.active}
                  onClick={() => setDialog({ kind: 'switch', label: s.label })}
                >
                  {s.active ? '当前' : '切换'}
                </Button>
              }
            >
              <span>{s.label}</span>
              {s.active && <Pill tone="accent">激活</Pill>}
              {!s.logged_in && <Pill tone="default">未登录</Pill>}
              {s.cli_days_left !== null && s.cli_days_left !== undefined && (
                <Pill
                  tone={s.cli_days_left < 0 ? 'danger' : s.cli_days_left < 5 ? 'warn' : 'ok'}
                >
                  {fmtDaysLeft(s.cli_days_left)}
                </Pill>
              )}
            </Row>
          ))
        ) : (
          <EmptyState
            icon={<KeyRound size={22} />}
            title="还没有账户槽位"
            action={
              <Button variant="primary" icon={<Terminal size={13} />} onClick={() => go('home')}>
                去总览启动 Claude Code 登录
              </Button>
            }
          >
            槽位是在你第一次登录之后建立的。先启动一次 Claude Code 并完成登录，
            这里就会出现对应的槽位。
          </EmptyState>
        )}

        {!!slots.length && (
          <Collapsible
            className="mt-2"
            summary="为什么过期的账户还能切？"
          >
            <p className="notice">
              <strong>「已登录」和「没过期」是两件事。</strong>
              凭证过期的账户<strong>必须仍然可切</strong> ——
              你得先切过去，才能在那个槽里重新登录。把这两件事合并，
              就等于把自己锁在门外。
            </p>
            <p className="notice mt-2">{accounts.data?.caveat}</p>
            <p className="notice mt-2">
              剩余天数只读 <code>refreshTokenExpiresAt</code> 这个本地时间戳，
              <strong>查不出「被风控下线」</strong>——
              唯一能确认的办法是实际发一次认证请求。
              accessToken 8–12 小时过期是正常的，客户端自己会静默换新，
              面板故意不为它报警。
            </p>
          </Collapsible>
        )}
      </Card>

      {/* ------------------------------------------------------ 登录 */}
      <Card title="登录" tone="accent" className="mb-3">
        <p className="notice mb-3">
          <code>claude.exe</code> 上常驻一条 Deny ExecuteFile，
          <strong>双击它会被系统拒绝执行，这是设计如此</strong>。
          登录必须走下面这两个受控入口 —— 它们会先验 IP 再放行并启动。
        </p>
        <div className="flex flex-wrap gap-2">
          <Button
            variant="primary"
            icon={<Terminal size={13} />}
            loading={busy === 'login-cli'}
            disabled={!!busy}
            onClick={() =>
              act('login-cli', () => api.launchClaude('claude-code'), 'Claude Code 已放行并启动')
            }
          >
            验证 IP 后打开 Claude Code
          </Button>
          <Button
            variant="primary"
            icon={<MonitorSmartphone size={13} />}
            loading={busy === 'login-desktop'}
            disabled={!!busy}
            onClick={() =>
              act('login-desktop', () => api.launchClaude('claude-desktop'), '桌面端已放行并启动')
            }
          >
            验证 IP 后打开桌面端
          </Button>
        </div>
      </Card>

      {/* ------------------------------------------------------ 租约 */}
      <Card title="租约" icon={<LockKeyhole size={14} />}>
        <Row
          side={
            <Button
              size="sm"
              variant="danger"
              disabled={!!busy || !holder}
              onClick={() => setDialog({ kind: 'release' })}
            >
              收回并上锁
            </Button>
          }
        >
          <span>当前租约</span>
          {holder ? <Pill tone="warn">已租给 {holder}</Pill> : <Pill tone="ok">无，全部锁着</Pill>}
        </Row>
        <p className="notice mt-2">
          租约在外时执行锁是摘掉的。收回租约会
          <strong>重锁全部副本</strong>，不只是租出去的那一条 ——
          自动更新写的新 exe 继承的是干净 ACL，只重锁租出的路径会漏掉它。
        </p>
      </Card>

      <div className="mt-4 flex justify-end">
        <Button
          variant="ghost"
          icon={<SkipForward size={13} />}
          onClick={async () => {
            await mark('accounts', 'skipped', 'unknown', '用户强制跳过，未确认账户状态');
            toast.info('已标记为跳过');
          }}
        >
          标记为已跳过
        </Button>
      </div>

      {/* ---------------------------------------------------- 确认框 */}

      <ConfirmDialog
        open={dialog?.kind === 'switch'}
        onCancel={() => setDialog(null)}
        onConfirm={() =>
          dialog?.kind === 'switch' &&
          act('switch', () => api.accountsSwitch(dialog.label), `已切换到 ${dialog.label}`)
        }
        title={`切换到 ${dialog?.kind === 'switch' ? dialog.label : ''}？`}
        confirmLabel="确认切换"
        loading={busy === 'switch'}
        danger={!!holder}
      >
        <p>
          切换会改动目录联结点，把 Claude 的配置目录指到另一个槽位。
          任意时刻只有一个账户是激活的。
        </p>
        {holder && (
          <p className="notice notice--danger mt-2">
            <strong>现在有一个在外的租约（{holder}）。</strong>
            正在跑的 Claude 会话仍然指着旧槽位，切换之后它的行为会变得不可预期。
            建议先收回租约再切。
          </p>
        )}
        <p className="notice mt-2">
          账户切换只能由你手动触发，面板没有定时器也没有自动调用点。
          <strong>所有账户必须是你本人拥有的。</strong>
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={dialog?.kind === 'release'}
        onCancel={() => setDialog(null)}
        onConfirm={() => act('release', api.gateRelease, '租约已收回，全部副本重新上锁')}
        title="收回租约并重新上锁？"
        confirmLabel="确认收回"
        loading={busy === 'release'}
        danger
      >
        <p>
          全部 claude.exe 副本会重新加上 Deny ExecuteFile。
          <strong>正在跑的会话不会被杀掉</strong>，但它下次启动会被拒绝。
        </p>
        <p className="notice mt-2">
          之后要再打开，得回到总览走一次受控启动。
        </p>
      </ConfirmDialog>
    </>
  );
}
