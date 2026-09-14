import { useState } from 'react';
import {
  Clock,
  Download,
  History as HistoryIcon,
  PackageCheck,
  RotateCw,
  Sparkles,
} from 'lucide-react';

import {
  api,
  type Channel,
  type InstallTarget,
  type Risk,
  type SoftwareReport,
  type Trace,
} from '../lib/api';
import type { ScanResult } from '../lib/signals';
import { markStep } from '../lib/progress';
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
  INSTALL_KIND_LABEL,
  INSTALL_TARGET_LABEL,
  UPGRADE_ACTION_LABEL,
  UPGRADE_ACTION_TONE,
} from '../ui';
import { ExternalsBlock, ManagedDirControl, VersionHistoryBlock } from './managed/ManagedPanel';

const DOWNLOAD_PAGE = 'https://claude.ai/download';
const CHROME_PAGE = 'https://www.google.com/chrome/';

/** 痕迹分类的中文名与语气。`credential` 与 `browser` 是「登录过」那一档。 */
const TRACE_LABEL: Record<Trace['kind'], string> = {
  credential: '凭证',
  config: '配置',
  install: '安装',
  registry: '注册表',
  browser: '浏览器',
};
const TRACE_TONE: Record<Trace['kind'], 'danger' | 'warn' | 'default'> = {
  credential: 'danger',
  browser: 'danger',
  config: 'warn',
  install: 'warn',
  registry: 'default',
};

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
  const toast = useToast();

  const sw = useResource('software', R.software);
  const install = useResource('install', R.install);
  const managed = useResource('managed', R.managed);
  const upgrade = useResource('upgrade', R.upgrade);
  const tzSys = useResource('tz', R.tz);
  const ip = useResource('ip', R.ip);

  const installTask = useTask('install');
  const upgradeTask = useTask('upgrade');
  const chromeTask = useTask('chrome-reinstall');
  const traces = useResource('traces', R.traces);

  const [busy, setBusy] = useState('');
  const [prompt, setPrompt] = useState<PromptDef | null>(null);
  const [channel, setChannel] = useSession<Channel>('env.channel', 'latest');
  const [restoreTz, setRestoreTz] = useSession('env.restoreTz', false);
  const [askInstall, setAskInstall] = useState<InstallTarget | null>(null);
  const [askTz, setAskTz] = useState(false);
  const [askChrome, setAskChrome] = useState(false);

  const ipTz = ip.data?.timezone ?? '';
  const browserTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const tzMismatch = !!ipTz && ipTz !== browserTz;
  const anyInstalled = !!(sw.data?.claudeCode.installed || sw.data?.claudeDesktop.installed);

  const tr = traces.data;
  /**
   * 「登录过」只认凭证文件与浏览器痕迹。
   *
   * **装过 ≠ 登录过。** 合并了就会对一台只装过、没登录过的机器建议清空浏览器，
   * 那是一次白白毁掉书签密码的操作。
   */
  const everLoggedInNow = !!tr?.traces.some(
    (t) => t.kind === 'credential' || t.kind === 'browser'
  );

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
    await markStep(
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

  /**
   * 清空并重装 Chrome。
   *
   * ⚠ **调用之前必须已经确认过。** 后端不会再问第二次 ——
   * 使用者要求确认之后全程自动、中途不再问，所以那一次确认框
   * （下面 `askChrome` 那个）必须把代价说全。
   */
  async function runChromeReinstall() {
    setBusy('chrome');
    resetTask('chrome-reinstall');
    try {
      const detail = await api.chromeReinstall();
      endTask('chrome-reinstall');
      toast.ok(detail);
      invalidate(...AFTER.browser);
      void traces.refresh();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      endTask('chrome-reinstall', msg);
      toast.error(msg);
    } finally {
      setBusy('');
      setAskChrome(false);
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

        {[sw.data?.claudeCode.advisory, sw.data?.claudeDesktop.advisory]
          .filter(Boolean)
          .map((a) => (
            <p key={a} className="notice notice--warn mt-2">
              {a}
            </p>
          ))}

        {/* 检测、启动、上锁、升级现在用同一张位置表（Rust install::inventory）。
            原来检测只看 ~\.local\bin，winget / npm / Scoop 装的一律报「未安装」。
            把整张表摊开给人看：哪份拿来启动、哪份锁得上、哪份锁不上。 */}
        {!!sw.data?.claudeCodeInstalls.length && (
          <Collapsible
            className="mt-2"
            summary={`本机共有 ${sw.data.claudeCodeInstalls.length} 份 Claude Code 副本`}
          >
            {sw.data.claudeCodeInstalls.map((i) => (
              <Row
                key={i.path}
                side={
                  <>
                    {i.preferred && <Pill tone="accent">启动用这份</Pill>}
                    {i.lockable ? (
                      <Pill tone="ok">执行锁管得到</Pill>
                    ) : (
                      <Pill tone="warn" title="批处理由 cmd.exe 读进去执行，给它加 Deny ExecuteFile 挡不住">
                        执行锁管不到
                      </Pill>
                    )}
                  </>
                }
              >
                <span>{INSTALL_KIND_LABEL[i.kind] ?? i.kind}</span>
                <span className="notice block w-full break-all font-mono">{i.path}</span>
              </Row>
            ))}
            <p className="notice mt-2">
              每一份都要锁上，漏掉一份就是一个绕过门禁的入口；只有「启动用这份」那一份会被面板拉起来。
            </p>
          </Collapsible>
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
        {/* v0.9.0：Claude Code 与 Codex 由面板装进自己的托管目录 —— 位置由面板说了算，
            面板再也不用满世界找它们。桌面端的位置被官方安装器写死，照旧走 winget。 */}
        <ManagedDirControl />
        <p className="notice mb-3">
          Claude Code 与 Codex 从官方源（<code>downloads.claude.ai</code>、
          <code>github.com/openai/codex</code>）下载最新版，核对官方给的 SHA-256 与数字签名后放进托管目录，
          装完<strong>自动重新上锁</strong>。已经装了就是升级，旧版留底。
        </p>

        <div className="grid gap-3 sm:grid-cols-2">
          {(['claude-code', 'codex'] as const).map((t) => {
            const st = managed.data?.apps.find((a) => a.app === t);
            return (
              <div key={t}>
                <Button
                  variant="primary"
                  block
                  icon={<Download size={13} />}
                  loading={busy === t}
                  disabled={!!busy}
                  onClick={() => setAskInstall(t)}
                >
                  {st?.installed ? '重新下载安装' : '安装'} {INSTALL_TARGET_LABEL[t]}
                </Button>
                <p className="notice mt-1.5 break-all">
                  {st?.installed ? (
                    <>
                      面板托管 {st.version ?? '版本未知'} · <code>{st.path}</code>
                    </>
                  ) : (
                    <>
                      将装到 <code>{st?.path ?? '托管目录'}</code>
                    </>
                  )}
                </p>
              </div>
            );
          })}

          {(() => {
            const t = 'claude-desktop' as const;
            const pkg = pkgOf(t);
            const installed = INSTALLED_OF[t](sw.data);
            const usable = wingetOk && pkg?.found;
            return (
              <div>
                {usable ? (
                  <Button
                    block
                    icon={<Download size={13} />}
                    loading={busy === t}
                    disabled={!!busy}
                    onClick={() => setAskInstall(t)}
                  >
                    {installed ? '重新安装' : '安装'} {INSTALL_TARGET_LABEL[t]}
                  </Button>
                ) : (
                  <ExternalLink href={DOWNLOAD_PAGE} asButton className="w-full justify-center">
                    打开官方页面
                  </ExternalLink>
                )}
                <p className="notice mt-1.5">
                  桌面端的位置由官方安装器决定（<code>%LOCALAPPDATA%\AnthropicClaude</code>），
                  它还会自己在那里更新，面板接管不了它的目录，装完会把实际位置记下来。
                  {usable
                    ? ` 走 winget：${pkg?.id}${pkg?.available_version ? ` · 源里是 ${pkg.available_version}` : ''}`
                    : wingetOk
                      ? ' winget 源里查不到它的包。'
                      : ' 本机没有 winget，请到官方页面手动安装。'}
                </p>
              </div>
            );
          })()}
        </div>

        <ExternalsBlock />
      </Card>

      {/* ---------------------------------------------------- 版本库 */}
      <Card title="版本库与回滚" icon={<HistoryIcon size={14} />} className="mb-3">
        <VersionHistoryBlock />

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

      </Card>

      {/* ------------------------------------- Claude 痕迹与 Chrome 重装 */}
      <Card
        title="以前装过 / 登录过 Claude 吗"
        icon={<HistoryIcon size={14} />}
        className="mb-3"
        actions={
          <Button size="sm" loading={traces.loading} onClick={() => void traces.refresh()}>
            扫描痕迹
          </Button>
        }
      >
        {/* 这一段原来藏在「一个 Claude 都没装」的条件里，而会关心这个问题的人
            机器上往往正装着 Claude —— 于是它对绝大多数人从来没显示过。
            现在跟装没装无关，只看扫到了什么。 */}
        {!tr ? (
          <p className="notice">
            还没扫过。点右上角「扫描痕迹」——
            会查凭证、配置目录、安装落点、注册表卸载项，以及 Chrome 的用户资料里
            有没有 claude.ai 的痕迹。<strong>只读，不改任何东西。</strong>
          </p>
        ) : (
          <>
            <div className="grid gap-2 sm:grid-cols-3">
              <Metric label="扫到的痕迹">
                {tr.traces.length > 0 ? (
                  <>
                    {tr.traces.length} 处
                    <Pill tone={tr.traces.length ? 'warn' : 'ok'}>
                      {everLoggedInNow ? '登录过' : '装过'}
                    </Pill>
                  </>
                ) : (
                  <Pill tone="ok">没扫到</Pill>
                )}
              </Metric>
              <Metric label="Google Chrome">
                {tr.chrome_installed ? (
                  <>
                    已安装
                    <Pill tone="ok">已装</Pill>
                  </>
                ) : (
                  <Pill tone="default">未安装</Pill>
                )}
              </Metric>
              <Metric label="Chrome 资料">
                {tr.chrome_running ? (
                  <Pill tone="warn">正在运行，没扫</Pill>
                ) : tr.chrome_scanned ? (
                  <Pill tone="ok">已扫过</Pill>
                ) : (
                  <Pill tone="default">没有可扫的</Pill>
                )}
              </Metric>
            </div>

            {/* 「没扫」和「没找到」是两回事。合并成一句「没有痕迹」，
                就是一个说谎的否定结论 —— 档案 §7.17 那条教训的同一类。 */}
            {tr.chrome_running && (
              <p className="notice notice--warn mt-2">
                <strong>Chrome 正在运行，它的资料文件被占着打不开，这一轮没扫。</strong>
                这不等于「没有痕迹」。关掉 Chrome 再点一次「扫描痕迹」。
              </p>
            )}

            {tr.traces.length > 0 && (
              <Collapsible className="mt-2" summary={`看看扫到了哪 ${tr.traces.length} 处`}>
                {tr.traces.map((t) => (
                  <Row key={`${t.kind}:${t.path}`} side={<Pill tone={TRACE_TONE[t.kind]}>{TRACE_LABEL[t.kind]}</Pill>}>
                    <span>{t.label}</span>
                    <span className="notice block w-full break-all font-mono">{t.path}</span>
                    <span className="notice block">{t.detail}</span>
                  </Row>
                ))}
              </Collapsible>
            )}

            {tr.traces.length === 0 && !tr.chrome_running && (
              <p className="notice mt-2">
                没扫到任何痕迹。这台机器看起来没装过、也没登录过 Claude。
              </p>
            )}

            {/* ---- 重装 Chrome */}
            <div className="mt-3 border-t border-line pt-3">
              <p className="notice">
                {tr.chrome_installed
                  ? '清空重装会先关掉 Chrome、用 winget 卸载、删掉整个用户资料目录，再装回来。'
                  : '本机没有 Chrome。点下面这个按钮会直接用 winget 装一个。'}
                <strong>
                  {tr.chrome_installed
                    ? '书签、密码、扩展、全部站点数据都会一起没掉，不可恢复。'
                    : ''}
                </strong>
                只碰 Google Chrome，Edge 与其它浏览器一概不动。
              </p>

              {!tr.winget_available ? (
                <p className="notice notice--warn mt-2">
                  本机没有 winget，自动重装这条路走不通。
                  <ExternalLink href={CHROME_PAGE} className="ml-1">
                    到官方页面手动处理
                  </ExternalLink>
                </p>
              ) : (
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <Button
                    variant={tr.chrome_installed ? 'danger' : 'primary'}
                    icon={<Download size={13} />}
                    loading={busy === 'chrome'}
                    disabled={!!busy}
                    onClick={() => setAskChrome(true)}
                  >
                    {tr.chrome_installed ? '清空并重装 Chrome' : '安装 Chrome'}
                  </Button>
                  {tr.chrome_path && (
                    <span className="notice break-all font-mono">{tr.chrome_path}</span>
                  )}
                </div>
              )}

              {(chromeTask.running || chromeTask.log.length > 0) && (
                <div className="mt-3">
                  <ProgressBar
                    value={
                      chromeTask.total > 0
                        ? (chromeTask.step / chromeTask.total) * 100
                        : undefined
                    }
                    tone={chromeTask.error ? 'danger' : 'accent'}
                    label="Chrome 重装进度"
                  />
                  <p className="notice mt-1">{chromeTask.phase}</p>
                  <div className="mt-2">
                    <LogView lines={chromeTask.log} follow={chromeTask.running} />
                  </div>
                </div>
              )}
            </div>
          </>
        )}

        <Collapsible className="mt-3" summary="想自己动手？这里有一份提示词">
          <p className="notice">
            上面那个按钮是全自动的。要自己一步步来（或者要连 Edge、Firefox 一起处理），
            可以照这份提示词交给 Codex —— 它会先提醒你备份书签与密码。
          </p>
          <Button className="mt-2" onClick={() => setPrompt(BROWSER_REINSTALL_PROMPT)}>
            查看浏览器重装提示词
          </Button>
        </Collapsible>
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
              {/* 「没装过」和「装着但读不出版本」是两回事。合成一句「未安装」，
                  就会像 2026-09-10 那次一样：218 MB 的 claude.exe 好端端在磁盘上，
                  面板却说没装 —— 排查方向从一开始就被带偏。 */}
              <Metric
                label="本机"
                emptyText={plan.action === 'version_unreadable' ? '读不出版本' : '未安装'}
              >
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
        {askInstall === 'claude-desktop' ? (
          <>
            <p>安装过程会依次做这几件事：</p>
            <ol className="mt-2 ml-4 list-decimal">
              <li>先摘掉全部执行锁，否则安装程序可能写不进去</li>
              <li>用 winget 装官方安装器，日志实时显示在下面</li>
              <li>重新枚举所有 claude.exe 副本，把桌面端实际装在哪记下来</li>
              <li>核对新文件的 Authenticode 签名主体</li>
              <li>
                <strong>重新上锁</strong>——这一步失败会报错，不会默默放过
              </li>
            </ol>
          </>
        ) : (
          <>
            <p>装进下面这个托管目录。要换位置就点「更改目录」—— 选的时候当场实测锁不锁得住：</p>
            <div className="mt-2">
              <ManagedDirControl compact disabled={!!busy} />
            </div>
            <p className="mt-3">依次做这几件事：</p>
            <ol className="mt-2 ml-4 list-decimal">
              <li>当场实测托管目录锁不锁得住</li>
              <li>查官方最新版本，下载（进度显示在下面）</li>
              <li>
                核对<strong>官方给的 SHA-256</strong>，对不上就删掉、不装
              </li>
              <li>
                核对数字签名：主体必须是 {askInstall === 'codex' ? 'OpenAI' : 'Anthropic'}，
                读不出也不装
              </li>
              <li>放进托管目录（已有的旧版改名留底），然后<strong>重新上锁</strong></li>
            </ol>
          </>
        )}
        <p className="notice mt-3">
          新装的 exe 继承的是干净 ACL，门禁那条 Deny 不会自己跟过去，所以最后那一步重锁是必须的。
        </p>
      </ConfirmDialog>

      <ConfirmDialog
        open={askChrome}
        onCancel={() => setAskChrome(false)}
        onConfirm={() => void runChromeReinstall()}
        title={tr?.chrome_installed ? '清空并重装 Google Chrome？' : '安装 Google Chrome？'}
        confirmLabel={tr?.chrome_installed ? '我知道会全部清空，开始' : '开始安装'}
        loading={busy === 'chrome'}
        danger={!!tr?.chrome_installed}
      >
        {tr?.chrome_installed ? (
          <>
            <p>
              <strong>这一步会毁掉数据，而且不可恢复。</strong>
              点下去之后全程自动，中途不会再问你。
            </p>
            <ol className="mt-2 ml-4 list-decimal">
              <li>强制关闭所有 Chrome 窗口（没保存的网页会丢）</li>
              <li>用 winget 卸载 Chrome</li>
              <li>
                删掉整个用户资料目录 <code>%LOCALAPPDATA%\Google\Chrome\User Data</code>
              </li>
              <li>用 winget 重新装一个干净的 Chrome</li>
            </ol>
            <p className="notice notice--danger mt-3">
              <strong>会一起没掉的：</strong>书签、保存的密码、自动填充、扩展及其数据、
              全部网站的 Cookie 与登录态 —— 不只是 claude.ai，是<strong>所有网站</strong>。
              要留书签或密码，请先关掉这个框，自己在 Chrome 里导出。
            </p>
            <p className="notice mt-2">
              第 3 步不是可选的：官方卸载程序<strong>默认不删这个目录</strong>，
              不删的话重装完旧的登录态原样还在，整件事白做。
            </p>
            <p className="notice mt-2">
              只碰 Google Chrome。Edge、Firefox 及其它浏览器一概不动。
              QB Gate 自己跑在 WebView2（Edge 内核）上，
              <strong>卸载 Chrome 不会影响这个面板</strong>。
            </p>
          </>
        ) : (
          <p>
            本机没有 Chrome，这一步只做安装：用 winget 装 <code>Google.Chrome</code>，
            不会碰任何现有数据。
          </p>
        )}
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
