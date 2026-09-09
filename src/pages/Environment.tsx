import { useState } from 'react';
import {
  Clock,
  Download,
  Fingerprint,
  PackageCheck,
  RotateCw,
  SkipForward,
  Sparkles,
} from 'lucide-react';

import {
  api,
  type Channel,
  type InstallTarget,
  type Risk,
  type SoftwareReport,
} from '../lib/api';
import type { ScanResult } from '../lib/signals';
import { useNav } from '../lib/nav';
import { AFTER, R } from '../lib/resources';
import { invalidate, peek, useResource, useSession } from '../lib/store';
import { endTask, resetTask, useTask } from '../lib/tasks';
import { BROWSER_REINSTALL_PROMPT, CLEAN_REINSTALL_PROMPT, type PromptDef } from '../prompts';
import {
  Button,
  Card,
  Checkbox,
  CodeBlock,
  Collapsible,
  ConfirmDialog,
  ExternalLink,
  LogView,
  Metric,
  PageHeader,
  Pill,
  ProgressBar,
  Row,
  useToast,
  CHANNEL_LABEL,
  INSTALL_TARGET_LABEL,
  UPGRADE_ACTION_LABEL,
  UPGRADE_ACTION_TONE,
} from '../ui';

const DOWNLOAD_PAGE = 'https://claude.ai/download';
const CODEX_DOWNLOAD_PAGE = 'https://github.com/openai/codex';

/** 每个安装目标去 `SoftwareReport` 的哪一项查「装没装」。 */
const INSTALLED_OF: Record<
  InstallTarget,
  (r: SoftwareReport | undefined) => boolean | undefined
> = {
  'claude-code': (r) => r?.claudeCode.installed,
  'claude-desktop': (r) => r?.claudeDesktop.installed,
  codex: (r) => r?.codex.installed,
};

export default function Environment() {
  const { mark, go } = useNav();
  const toast = useToast();

  const sw = useResource('software', R.software);
  const install = useResource('install', R.install);
  const upgrade = useResource('upgrade', R.upgrade);
  const tzSys = useResource('tz', R.tz);
  const ip = useResource('ip', R.ip);

  const installTask = useTask('install');
  const upgradeTask = useTask('upgrade');

  const [busy, setBusy] = useState('');
  const [prompt, setPrompt] = useState<PromptDef | null>(null);
  const [channel, setChannel] = useSession<Channel>('env.channel', 'latest');
  const [everLoggedIn, setEverLoggedIn] = useSession<'unset' | 'yes' | 'no'>(
    'env.everLoggedIn',
    'unset'
  );
  const [restoreTz, setRestoreTz] = useSession('env.restoreTz', false);
  const [askInstall, setAskInstall] = useState<InstallTarget | null>(null);
  const [askTz, setAskTz] = useState(false);

  const ipTz = ip.data?.timezone ?? '';
  const browserTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const tzMismatch = !!ipTz && ipTz !== browserTz;
  const anyInstalled = !!(sw.data?.claudeCode.installed || sw.data?.claudeDesktop.installed);

  const wingetOk = install.data?.winget_available ?? false;
  function pkgOf(t: InstallTarget) {
    return install.data?.packages.find((p) => p.target === t);
  }

  async function recordRisk() {
    // 中文环境识别拆到了独立页，但它仍然归在 `environment` 这一步里 ——
    // 已经扫过就把结果并进来，没扫过就只看软件与时区。
    const scan = peek<ScanResult>('signals');
    const s = peek<SoftwareReport>('software');
    const risk: Risk =
      scan?.band === 'high'
        ? 'high'
        : scan?.band === 'medium' || tzMismatch || !s?.claudeCode.installed
          ? 'medium'
          : 'low';
    await mark(
      'environment',
      risk === 'high' ? 'failed' : 'passed',
      risk,
      [
        s?.claudeCode.installed ? 'Claude Code 已装' : 'Claude Code 未装',
        s?.codex.installed ? 'Codex 已装' : 'Codex 未装',
        tzMismatch ? '时区与出口 IP 不一致' : '时区一致',
        scan ? `中文环境 ${scan.total}/100` : null,
      ]
        .filter(Boolean)
        .join('；')
    );
  }

  async function runInstall(target: InstallTarget) {
    setBusy(target);
    resetTask('install');
    try {
      const r = await api.installRun(target);
      endTask('install', r.ok ? undefined : r.detail);
      if (r.ok) {
        toast.ok(`${INSTALL_TARGET_LABEL[target]} 安装完成，重新上锁 ${r.relocked} 个副本`);
        if (r.signature_ok === false) {
          toast.error('注意：新文件的签名主体里没有 Anthropic，请自行核实来源');
        }
      } else {
        toast.error(r.detail);
      }
      invalidate(...AFTER.install);
      await recordRisk();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask('install', msg);
      toast.error(msg);
    } finally {
      setBusy('');
      setAskInstall(null);
    }
  }

  async function runUpgrade() {
    setBusy('upgrade');
    resetTask('upgrade');
    try {
      const detail = await api.upgradeExecute(channel, false);
      endTask('upgrade');
      toast.ok(detail);
      invalidate(...AFTER.install);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask('upgrade', msg);
      toast.error(msg);
    } finally {
      setBusy('');
    }
  }

  const plan = upgrade.data;

  return (
    <>
      <PageHeader
        title="环境与安装"
        sub="检测本机是否已装 Claude、安装与升级、系统时区对齐。"
        actions={
          <Button
            icon={<RotateCw size={13} />}
            loading={sw.loading}
            onClick={() => {
              void sw.refresh();
              void install.refresh();
            }}
          >
            重新检测
          </Button>
        }
      />

      {/* ---------------------------------------------------- 本机软件 */}
      <Card title="本机软件" className="mb-3">
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">
          {[sw.data?.claudeDesktop, sw.data?.claudeCode, sw.data?.codex].map(
            (s, i) =>
              s && (
                <Metric key={s.id ?? i} label={s.name}>
                  {s.installed ? (
                    <>
                      {s.version ?? '已安装'}
                      <Pill tone="ok">已装</Pill>
                    </>
                  ) : (
                    <Pill tone="default">未安装</Pill>
                  )}
                </Metric>
              )
          )}
          <Metric label="浏览器" loading={sw.loading && !sw.data}>
            {sw.data ? `${sw.data.browsers.filter((b) => b.installed).length} 个` : undefined}
          </Metric>
        </div>

        {sw.loading && !sw.data && (
          <div className="grid gap-2 sm:grid-cols-4">
            <Metric label="Claude 桌面端" loading />
            <Metric label="Claude Code" loading />
          </div>
        )}
      </Card>

      {/* ------------------------------------------------------ 安装 */}
      <Card
        title="安装"
        icon={<Download size={14} />}
        className="mb-3"
        actions={
          wingetOk ? (
            <Pill tone="ok">winget 可用</Pill>
          ) : install.data ? (
            <Pill tone="warn">没有 winget</Pill>
          ) : null
        }
      >
        {wingetOk ? (
          <p className="notice mb-3">
            走 winget 安装，完整性由官方 manifest 保证；
            Claude Code 若失败会自动回退到官方安装脚本。
            装完会重新枚举副本并<strong>自动重新上锁</strong>。
          </p>
        ) : (
          <p className="notice notice--warn mb-3">
            本机没有 winget，面板不再自己下载安装包 ——
            请到官方下载页手动安装，装完回来点一次「重新检测」。
          </p>
        )}

        <div className="grid gap-3 sm:grid-cols-2">
          {(['claude-code', 'claude-desktop', 'codex'] as const).map((t) => {
            const pkg = pkgOf(t);
            const installed = INSTALLED_OF[t](sw.data);
            const usable = wingetOk && pkg?.found;
            return (
              <div key={t}>
                {usable ? (
                  <Button
                    variant="primary"
                    block
                    icon={<Download size={13} />}
                    loading={busy === t}
                    disabled={!!busy}
                    onClick={() => setAskInstall(t)}
                  >
                    {installed ? '重新安装' : '安装'} {INSTALL_TARGET_LABEL[t]}
                  </Button>
                ) : (
                  <ExternalLink
                    href={t === 'codex' ? CODEX_DOWNLOAD_PAGE : DOWNLOAD_PAGE}
                    asButton
                    className="w-full justify-center"
                  >
                    打开官方页面
                  </ExternalLink>
                )}
                <p className="notice mt-1.5">
                  {usable ? (
                    <>
                      <code>{pkg?.id}</code>
                      {pkg?.available_version ? ` · 源里是 ${pkg.available_version}` : ''}
                    </>
                  ) : wingetOk ? (
                    `winget 源里查不到 ${INSTALL_TARGET_LABEL[t]} 的包`
                  ) : (
                    '需要 winget，或者手动下载安装'
                  )}
                </p>
              </div>
            );
          })}
        </div>

        {(installTask.running || installTask.log.length > 0) && (
          <div className="mt-4">
            <div className="mb-1.5 flex items-center gap-2">
              <span className="text-sm">{installTask.phase || '安装中…'}</span>
              {installTask.total > 0 && (
                <span className="notice ml-auto">
                  {installTask.step} / {installTask.total}
                </span>
              )}
            </div>
            <ProgressBar
              value={
                installTask.total > 0 ? (installTask.step / installTask.total) * 100 : undefined
              }
              tone={installTask.error ? 'danger' : 'accent'}
              label="安装进度"
            />
            <div className="mt-2">
              <LogView lines={installTask.log} follow={installTask.running} />
            </div>
          </div>
        )}

        {anyInstalled && (
          <Collapsible className="mt-3" summary="当前安装有问题？做一次完整卸载重装">
            <p className="notice">
              配置写坏、装了一半、要转交机器 —— 这几种情况建议整套清掉重来。
              交给 Codex 按提示词一步步做，它会先盘点再确认才动手。
            </p>
            <div className="mt-2">
              <Button variant="danger" onClick={() => setPrompt(CLEAN_REINSTALL_PROMPT)}>
                查看卸载重装提示词
              </Button>
            </div>
          </Collapsible>
        )}

        {!anyInstalled && (
          <Collapsible className="mt-3" summary="以前在这台机器上登录过 Claude 吗？">
            <p className="notice">
              登录过的话，浏览器里可能还留着旧的登录态与站点数据。
            </p>
            <div className="mt-2 flex flex-wrap gap-2">
              <Button
                variant={everLoggedIn === 'yes' ? 'primary' : 'default'}
                onClick={() => setEverLoggedIn('yes')}
              >
                登录过
              </Button>
              <Button
                variant={everLoggedIn === 'no' ? 'primary' : 'default'}
                onClick={() => setEverLoggedIn('no')}
              >
                没有
              </Button>
            </div>
            {everLoggedIn === 'yes' && (
              <div className="mt-3">
                <p className="notice notice--warn">
                  建议重装浏览器再登录。提示词里会先提醒你备份书签与密码。
                </p>
                <Button className="mt-2" onClick={() => setPrompt(BROWSER_REINSTALL_PROMPT)}>
                  查看浏览器重装提示词
                </Button>
              </div>
            )}
          </Collapsible>
        )}
      </Card>

      {prompt && (
        <Card
          title={prompt.title}
          tone="accent"
          className="mb-3"
          actions={
            <Button size="sm" variant="ghost" onClick={() => setPrompt(null)}>
              收起
            </Button>
          }
        >
          <p className="notice mb-2">{prompt.modelHint}</p>
          <CodeBlock text={prompt.body} caption={prompt.title} />
        </Card>
      )}

      {/* ------------------------------------------------------ 升级 */}
      <Card
        title="升级 Claude Code"
        icon={<Sparkles size={14} />}
        className="mb-3"
        actions={
          <Button size="sm" loading={upgrade.loading} onClick={() => void upgrade.refresh()}>
            检查版本
          </Button>
        }
      >
        {plan ? (
          <>
            <div className="grid gap-2 sm:grid-cols-3">
              <Metric label="本机" emptyText="未安装">
                {plan.installed}
              </Metric>
              <Metric label={`${CHANNEL_LABEL[channel]}渠道`} emptyText="查不到">
                {plan.available}
              </Metric>
              <Metric label="建议">
                <Pill tone={UPGRADE_ACTION_TONE[plan.action]}>
                  {UPGRADE_ACTION_LABEL[plan.action]}
                </Pill>
              </Metric>
            </div>
            <p className="notice mt-2">{plan.detail}</p>
          </>
        ) : (
          <p className="notice">还没查过版本。点右上角「检查版本」。</p>
        )}

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <label className="notice" htmlFor="env-channel">
            渠道
          </label>
          <select
            id="env-channel"
            className="input w-auto"
            value={channel}
            onChange={(e) => {
              setChannel(e.target.value as Channel);
              void upgrade.refresh();
            }}
          >
            <option value="latest">{CHANNEL_LABEL.latest}（推荐）</option>
            <option value="stable">{CHANNEL_LABEL.stable}</option>
          </select>
          <Button
            variant="primary"
            loading={busy === 'upgrade'}
            disabled={
              !!busy || !plan || plan.action === 'up_to_date' || plan.action === 'would_downgrade'
            }
            onClick={runUpgrade}
          >
            升级
          </Button>
        </div>

        {(upgradeTask.running || upgradeTask.log.length > 0) && (
          <div className="mt-3">
            <ProgressBar
              value={
                upgradeTask.total > 0 ? (upgradeTask.step / upgradeTask.total) * 100 : undefined
              }
              tone={upgradeTask.error ? 'danger' : 'accent'}
              label="升级进度"
            />
            <p className="notice mt-1">{upgradeTask.phase}</p>
            <div className="mt-2">
              <LogView lines={upgradeTask.log} follow={upgradeTask.running} />
            </div>
          </div>
        )}

        <Collapsible className="mt-2" summary="为什么默认是最新版而不是稳定版？">
          <p className="notice">
            实测稳定版可能比本机还旧（stable 2.1.236 而本机 2.1.258），
            照着装就是降级，面板会直接拦下来。
          </p>
          <p className="notice mt-2">
            版本号统一取<strong>前三段</strong>再比：本机文件属性读出来是四段的
            <code>2.1.258.0</code>，渠道接口返回三段的 <code>2.1.258</code>，
            按四段比会得出「渠道比本机旧」的错误结论。
          </p>
          <p className="notice mt-2">
            本机版本从<strong>文件属性</strong>读，不去运行 <code>claude.exe</code>——
            没有租约时它上面挂着 Deny ExecuteFile，升级流程不能依赖
            「能把这个二进制启动起来」。
          </p>
        </Collapsible>
      </Card>

      {/* ---------------------------------------------------- 时区 */}
      <Card title="时区对齐" icon={<Clock size={14} />} className="mb-3">
        <div className="grid gap-2 sm:grid-cols-3">
          <Metric label="出口 IP 时区" loading={ip.loading && !ip.data} error={ip.error}>
            {ipTz || undefined}
          </Metric>
          <Metric label="当前系统时区" loading={tzSys.loading && !tzSys.data} error={tzSys.error}>
            {tzSys.data}
          </Metric>
          <Metric label="浏览器读到的">
            {browserTz}
            {ipTz && (tzMismatch ? <Pill tone="danger">不一致</Pill> : <Pill tone="ok">一致</Pill>)}
          </Metric>
        </div>

        <div className="mt-3">
          <Checkbox checked={restoreTz} onChange={setRestoreTz}>
            退出面板时还原为原时区
            <span className="notice ml-1">（默认不勾选）</span>
          </Checkbox>
          <p className="notice mt-1">
            专机长期跑 Claude 时，时区保持一致比来回切更稳，所以默认不还原。
          </p>
        </div>

        <Button
          className="mt-3"
          variant="primary"
          disabled={!ipTz || !tzMismatch || !!busy}
          onClick={() => setAskTz(true)}
        >
          {tzMismatch ? `切换到 ${ipTz}` : '已经一致，无需切换'}
        </Button>
      </Card>

      {/* -------------------------------------------- 中文环境识别入口 */}
      <Card title="中文环境识别" icon={<Fingerprint size={14} />} className="mb-3">
        <Row
          side={
            <Button size="sm" onClick={() => go('signals')}>
              打开
            </Button>
          }
        >
          <span>十项加权指纹，满分 100，全部在本地算。</span>
        </Row>
      </Card>

      <div className="mt-4 flex flex-wrap justify-end gap-2">
        <Button
          icon={<PackageCheck size={13} />}
          onClick={async () => {
            await recordRisk();
            toast.ok('已记录环境检测结果');
          }}
        >
          记录本步结果
        </Button>
        <Button
          variant="ghost"
          icon={<SkipForward size={13} />}
          onClick={async () => {
            await mark('environment', 'skipped', 'unknown', '用户强制跳过，未确认本机环境');
            toast.info('已标记为跳过');
          }}
        >
          标记为已跳过
        </Button>
      </div>

      {/* ---------------------------------------------------- 确认框 */}

      <ConfirmDialog
        open={askInstall !== null}
        onCancel={() => setAskInstall(null)}
        onConfirm={() => askInstall && runInstall(askInstall)}
        title={`安装 ${askInstall ? INSTALL_TARGET_LABEL[askInstall] : ''}？`}
        confirmLabel="开始安装"
        loading={!!busy}
      >
        <p>安装过程会依次做这几件事：</p>
        <ol className="mt-2 ml-4 list-decimal">
          <li>先摘掉全部执行锁，否则安装程序可能写不进去</li>
          <li>用 winget 安装，日志实时显示在下面</li>
          <li>重新枚举所有 claude.exe 副本</li>
          <li>核对新文件的 Authenticode 签名主体</li>
          <li>
            <strong>重新上锁</strong>——这一步失败会报错，不会默默放过
          </li>
        </ol>
        <p className="notice mt-3">
          新装的 exe 继承的是干净 ACL，门禁那条 Deny 不会自己跟过去，
          所以第 5 步是必须的。
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={askTz}
        onCancel={() => setAskTz(false)}
        onConfirm={async () => {
          setBusy('tz');
          try {
            await api.tzApply(ipTz, restoreTz);
            toast.ok(`系统时区已切换到 ${ipTz}`);
            invalidate('tz');
          } catch (e) {
            toast.error(e instanceof Error ? e.message : String(e));
          } finally {
            setBusy('');
            setAskTz(false);
          }
        }}
        title={`把系统时区切换到 ${ipTz}？`}
        confirmLabel="确认切换"
        loading={busy === 'tz'}
      >
        <p>
          这会改<strong>整个系统</strong>的时区，不只是 Claude ——
          日历、日志时间戳、其它软件都会跟着变。
        </p>
        <p className="notice mt-2">
          需要管理员权限，<strong>会弹 UAC</strong>。
          {restoreTz
            ? '你勾了「退出时还原」，面板关闭时会切回去。'
            : '你没有勾「退出时还原」，切过去就一直是这个时区。'}
        </p>
      </ConfirmDialog>
    </>
  );
}
