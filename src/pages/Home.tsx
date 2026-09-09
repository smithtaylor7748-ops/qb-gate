import { useState } from 'react';
import {
  AlertTriangle,
  Flag,
  MonitorSmartphone,
  RotateCw,
  ShieldAlert,
  Square,
  Terminal,
  Wine,
} from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';

import { api, type KillReport } from '../lib/api';
import { useNav } from '../lib/nav';
import { AFTER, R } from '../lib/resources';
import { invalidate, useResource, useSession } from '../lib/store';
import { endTask, resetTask, useTask } from '../lib/tasks';
import { STEPS } from '../lib/steps';
import {
  Button,
  Bullet,
  Card,
  ConfirmDialog,
  Metric,
  PageHeader,
  Pill,
  ProgressBar,
  Row,
  useToast,
  EVIDENCE_LABEL,
  fmtDaysLeft,
} from '../ui';

export default function Home() {
  const { go, progress } = useNav();
  const toast = useToast();

  const ip = useResource('ip', R.ip);
  const gate = useResource('gate', R.gate);
  const accounts = useResource('accounts', R.accounts);
  const criteria = useResource('criteria', R.criteria);
  const plugins = useResource('plugins', R.plugins);

  const tavern = useTask('tavern-start');
  const launchCodeTask = useTask('launch-claude-code');
  const launchDesktopTask = useTask('launch-claude-desktop');
  const killPreviewTask = useTask('killswitch-preview');
  const killExecuteTask = useTask('killswitch-execute');

  const [busy, setBusy] = useState<string>('');
  const [askDesktop, setAskDesktop] = useState(false);
  const [kill, setKill] = useState<KillReport | null>(null);
  const [killing, setKilling] = useState(false);
  const [bannerOff, setBannerOff] = useSession('home.banner.off', false);

  function taskProgress(task: ReturnType<typeof useTask>, label: string) {
    if (!task.running && !task.error && !task.finished) return null;
    const value = task.total > 0 ? (task.step / task.total) * 100 : undefined;
    return (
      <div className="mt-2">
        <div className="mb-1 flex items-center gap-2">
          <span className="notice">{task.phase || label}</span>
          {task.total > 0 && <span className="notice ml-auto">{task.step} / {task.total}</span>}
        </div>
        <ProgressBar value={value} tone={task.error ? 'danger' : 'accent'} label={`${label}进度`} />
        {task.error && <p className="notice notice--danger mt-1">{task.error}</p>}
      </div>
    );
  }

  const purityRec = progress.steps['purity'];
  const purityFailed = purityRec?.state === 'failed' || purityRec?.risk === 'high';

  const untouched = STEPS.filter((s) => !progress.steps[s.id]).length;
  const skipped = Object.values(progress.steps).filter((s) => s.state === 'skipped').length;

  const active = accounts.data?.slots.find((s) => s.active);
  const locked = gate.data?.targets.filter((t) => t.locked).length ?? 0;
  const total = gate.data?.targets.length ?? 0;
  const running = plugins.data?.[0]?.state === 'running';
  const claudeUsageUrl = 'https://claude.ai/settings/usage';

  // ------------------------------------------------------------ 启动

  async function launch(target: 'claude-code' | 'claude-desktop') {
    setBusy(target);
    resetTask(target === 'claude-code' ? 'launch-claude-code' : 'launch-claude-desktop');
    try {
      const r = await api.launchClaude(target);
      endTask(target === 'claude-code' ? 'launch-claude-code' : 'launch-claude-desktop');
      toast.ok(r.detail);
      invalidate(...AFTER.lease);
    } catch (e) {
      // 门禁不过就一个进程都不起 —— 这里的错误原文已经说清是 IP 不在白名单
      // 还是根本查不到 IP，两者必须分开，不能合并成「启动失败」。
      const msg = e instanceof Error ? e.message : String(e);
      endTask(target === 'claude-code' ? 'launch-claude-code' : 'launch-claude-desktop', msg);
      toast.error(msg);
    } finally {
      setBusy('');
      setAskDesktop(false);
    }
  }

  async function launchTavern() {
    setBusy('tavern');
    resetTask('tavern-start');
    try {
      const url = await api.pluginStart();
      endTask('tavern-start');
      // 无论是新起的还是复用现有服务都要打开页面 ——
      // 少了这一步，成功的启动和崩溃看起来一模一样。
      if (url.startsWith('http')) await openUrl(url);
      toast.ok('酒馆已就绪，已打开页面');
      invalidate(...AFTER.tavern);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask('tavern-start', msg);
      toast.error(msg);
    } finally {
      setBusy('');
    }
  }

  async function previewKill() {
    setBusy('kill');
    resetTask('killswitch-preview');
    try {
      setKill(await api.killswitchPreview());
      endTask('killswitch-preview');
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask('killswitch-preview', msg);
      toast.error(msg);
    } finally {
      setBusy('');
    }
  }

  async function doKill() {
    setKilling(true);
    resetTask('killswitch-execute');
    try {
      const r = await api.killswitchExecute();
      endTask('killswitch-execute');
      toast.ok(
        `收掉 ${r.killed.length} 个进程，重新上锁 ${r.relocked} 个可执行文件` +
          (r.failed.length ? `，${r.failed.length} 个没收掉` : '')
      );
      setKill(null);
      invalidate(...AFTER.gate, 'plugins');
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask('killswitch-execute', msg);
      toast.error(msg);
    } finally {
      setKilling(false);
    }
  }

  // ------------------------------------------------------------ 渲染

  const info = ip.data;
  const place = [
    info?.isResidential === true ? '住宅' : info?.isResidential === false ? '非住宅' : '住宅未知',
    // 「原生 IP」公开接口就是没有这个字段，如实报未知，不猜成通过。
    '原生未知',
    info?.city || info?.region || info?.country || null,
    info?.timezone || null,
  ].filter(Boolean);

  return (
    <>
      <PageHeader
        title="总览"
        sub="上面看状态，下面点启动。"
        actions={
          <Button
            icon={<RotateCw size={13} />}
            loading={ip.loading}
            onClick={() => {
              void ip.refresh();
              void gate.refresh();
            }}
          >
            重新检测
          </Button>
        }
      />

      {/* ---------------------------------------------------------- 出口 */}
      <Card title="出口" className="mb-3">
        {ip.error ? (
          <div className="flex items-center gap-2">
            <AlertTriangle size={14} className="text-[var(--danger)]" aria-hidden="true" />
            <span className="text-[var(--danger)] text-sm">查不到出口 IP：{ip.error}</span>
            <Button size="sm" className="ml-auto" onClick={() => void ip.refresh()}>
              重试
            </Button>
          </div>
        ) : (
          <>
            <div className="font-mono text-[19px] leading-tight break-all">
              {ip.loading && !info ? (
                <span className="skeleton block h-6 w-52" aria-hidden="true" />
              ) : (
                (info?.ip ?? '—')
              )}
            </div>
            <p className="notice mt-1">{place.join(' · ')}</p>

            <div className="mt-3 flex flex-wrap items-center gap-2">
              <Pill tone={purityFailed ? 'danger' : purityRec?.risk === 'low' ? 'ok' : 'default'}>
                纯净度{' '}
                {info?.fraudScore ?? '未知'}
                {purityRec?.risk === 'low' ? ' 通过' : purityFailed ? ' 不合格' : ' 未复核'}
              </Pill>
              <Pill tone={total > 0 && locked === total ? 'ok' : 'warn'}>
                IP 锁 {gate.data ? `${locked}/${total}` : '—'}
              </Pill>
              <Pill
                tone={
                  !active
                    ? 'default'
                    : !active.logged_in
                      ? 'danger'
                      : (active.cli_days_left ?? 99) < 5
                        ? 'warn'
                        : 'ok'
                }
              >
                账户 {active ? (active.logged_in ? fmtDaysLeft(active.cli_days_left) : '未登录') : '无槽位'}
              </Pill>
            </div>
          </>
        )}
      </Card>

      <Card title="Claude 订阅" className="mb-3" tone="accent">
        <div className="grid gap-2 sm:grid-cols-3">
          <Metric label="当前账户">{active?.label ?? '无激活槽位'}</Metric>
          <Metric label="登录状态">
            {active ? (
              active.logged_in ? <Pill tone="ok">已登录</Pill> : <Pill tone="danger">未登录</Pill>
            ) : <Pill>未知</Pill>}
          </Metric>
          <Metric label="凭证状态" hint="只读本地 refresh token 时间戳">
            {active ? fmtDaysLeft(active.cli_days_left) : '—'}
          </Metric>
        </div>
        <p className="notice mt-3">
          Claude 的订阅用量由官方统一计算，ClaudeGate 不读取额度、429 或内部接口。
          请在 Claude 官方 Usage 页面查看当前套餐、剩余用量和重置时间。
        </p>
        <Button className="mt-2" variant="primary" onClick={() => void openUrl(claudeUsageUrl)}>
          打开 Claude 官方 Usage 页面
        </Button>
      </Card>

      {/* ------------------------------------------------- 未走完的检查项 */}
      {!bannerOff && (untouched > 0 || skipped > 0) && (
        <div className="banner banner--accent mb-3">
          <Flag size={14} className="banner-icon" aria-hidden="true" />
          <div className="banner-body">
            {untouched > 0 && `还有 ${untouched} 项没检查过。`}
            {skipped > 0 && `你跳过了 ${skipped} 项。`}
          </div>
          <div className="flex flex-shrink-0 gap-1.5">
            <Button size="sm" variant="primary" onClick={() => go('purity')}>
              去看看
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setBannerOff(true)}>
              不再提醒
            </Button>
          </div>
        </div>
      )}

      {/* ------------------------------------------------------- 爆红 */}
      {purityFailed && (
        <Card tone="danger" className="mb-3">
          <div className="flex items-start gap-2">
            <ShieldAlert size={16} className="mt-0.5 flex-shrink-0" aria-hidden="true" />
            <div className="min-w-0">
              <div className="text-md text-[var(--danger)]">IP 不合格</div>
              <p className="notice notice--danger mt-1">
                {purityRec?.detail || '三项硬指标至少缺一：纯净度、原生 IP、住宅 IP。'}
                继续用这条 IP 登录，风险由你自己承担。
              </p>
              <div className="mt-2 flex flex-wrap gap-2">
                <Button
                  variant="danger"
                  onClick={() => criteria.data && openUrl(criteria.data.iproyal)}
                  disabled={!criteria.data}
                >
                  前往 IPRoyal 购买住宅 IP
                </Button>
                <Button onClick={() => go('purity')}>重新复核</Button>
              </div>
            </div>
          </div>
        </Card>
      )}

      {/* ------------------------------------------------------- 启动 */}
      <Card title="启动">
        <p className="notice mb-3">
          三个启动都先校验出口 IP：<strong>门禁不过，一个进程都不会起</strong>。
          放行之后面板会在后台挂上看门狗。
        </p>

        <div className="grid gap-3 md:grid-cols-2">
          <div>
            <Button
              variant="primary"
              launch
              icon={<Terminal size={15} />}
              loading={busy === 'claude-code'}
              disabled={!!busy}
              onClick={() => launch('claude-code')}
            >
              启动 Claude Code
            </Button>
            <p className="notice mt-1.5">
              看门狗每 15 秒查一次。查不到 IP 会先上锁保住进程，180 秒后才关停。
            </p>
            {taskProgress(launchCodeTask, 'Claude Code 启动中…')}
          </div>

          <div>
            <Button
              variant="primary"
              launch
              icon={<MonitorSmartphone size={15} />}
              loading={busy === 'claude-desktop'}
              disabled={!!busy}
              onClick={() => setAskDesktop(true)}
            >
              启动 Claude 桌面端
            </Button>
            {/* 这段代价必须写在按钮下面，不能藏起来 —— 它不是 bug 是设计，
                但用户有权在点之前就知道。 */}
            <p className="notice notice--warn mt-1.5">
              看门狗每 20 秒查一次，查不到 IP <strong>立即关闭，不给宽限</strong>。
              VPN 重连或查询服务限流会直接关掉正在用的窗口。
            </p>
            {taskProgress(launchDesktopTask, 'Claude 桌面端启动中…')}
          </div>

          <div>
            <Button
              variant="primary"
              launch
              icon={<Wine size={15} />}
              loading={busy === 'tavern'}
              disabled={!!busy}
              onClick={launchTavern}
            >
              {running ? '打开酒馆 SillyTavern' : '启动酒馆 SillyTavern'}
            </Button>
            <p className="notice mt-1.5">
              {running
                ? '已经在跑了。再点一次只打开页面，不会重复起进程。'
                : '起桥接与酒馆，最长 80 秒。端口被别人占住会报错退出，不会去动无关进程。'}
            </p>
            {taskProgress(tavern, '酒馆启动中…')}
          </div>

          <div className="flex flex-col justify-end">
            <Button
              variant="danger"
              launch
              icon={<Square size={14} />}
              loading={busy === 'kill'}
              disabled={!!busy}
              onClick={previewKill}
            >
              一键关闭所有 Claude
            </Button>
            <p className="notice notice--danger mt-1.5">
              只收满足双重证据的进程，绝不按进程名杀。
              <strong>会连这个面板正在服务的 Claude Code 会话一起收掉。</strong>
            </p>
            {taskProgress(killPreviewTask, '扫描进程中…')}
          </div>
        </div>

        {taskProgress(killExecuteTask, '关闭进程中…')}
      </Card>

      {/* --------------------------------------------------- 门禁小结 */}
      <div className="mt-3 grid gap-2 sm:grid-cols-3">
        <Metric
          label="执行锁"
          loading={gate.loading && !gate.data}
          error={gate.error}
          onRetry={() => void gate.refresh()}
          emptyHint="还没读到门禁状态"
        >
          {gate.data
            ? gate.data.lease.holder
              ? `已放行给 ${gate.data.lease.holder}`
              : `${locked} / ${total} 已锁`
            : undefined}
        </Metric>
        <Metric
          label="看门狗"
          loading={gate.loading && !gate.data}
          error={gate.error}
          onRetry={() => void gate.refresh()}
        >
          {gate.data ? (gate.data.watchdog_running ? '运行中' : '未运行') : undefined}
        </Metric>
        <Metric
          label="残留副本"
          loading={gate.loading && !gate.data}
          error={gate.error}
          onRetry={() => void gate.refresh()}
          hint="升级留下的旧副本没有执行锁，是能绕过门禁的入口"
        >
          {gate.data ? (
            <>
              {gate.data.stale_copies.length} 个
              {gate.data.stale_copies.length > 0 && <Pill tone="danger">可绕过</Pill>}
            </>
          ) : undefined}
        </Metric>
      </div>

      {/* --------------------------------------------------- 确认框 */}

      <ConfirmDialog
        open={askDesktop}
        onCancel={() => setAskDesktop(false)}
        onConfirm={() => launch('claude-desktop')}
        title="启动 Claude 桌面端？"
        confirmLabel="我知道了，启动"
        loading={busy === 'claude-desktop'}
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

      <ConfirmDialog
        open={kill !== null}
        onCancel={() => setKill(null)}
        onConfirm={doKill}
        title="一键关闭所有 Claude"
        confirmLabel={`确认关闭 ${kill?.targets.length ?? 0} 个进程`}
        loading={killing}
        danger
      >
        {kill?.targets.length ? (
          <>
            <p>下面这些进程会被收掉，收完自动重新上锁：</p>
            <div className="mt-2">
              {kill.targets.map((t) => (
                <Row
                  key={t.pid}
                  side={<Pill tone="danger">{EVIDENCE_LABEL[t.evidence] ?? t.evidence}</Pill>}
                >
                  <span className="font-mono">
                    PID {t.pid} · {t.name}
                  </span>
                  <span className="notice block w-full break-all font-mono">{t.path ?? '路径未知'}</span>
                </Row>
              ))}
            </div>
            <p className="notice notice--danger mt-3">
              其中很可能包含<strong>正在为你运行这个面板的 Claude Code 会话</strong>。
            </p>
          </>
        ) : (
          <p>没有满足双重证据的进程，什么都不会动。</p>
        )}

        {!!kill?.spared.length && (
          <div className="mt-3">
            <p className="notice">放过了 {kill.spared.length} 个：</p>
            {kill.spared.slice(0, 6).map((s) => (
              <Bullet key={s}>{s}</Bullet>
            ))}
            {kill.spared.length > 6 && (
              <Bullet>…另有 {kill.spared.length - 6} 个同样不满足证据</Bullet>
            )}
          </div>
        )}
      </ConfirmDialog>
    </>
  );
}
