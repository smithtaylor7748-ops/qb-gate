import { Component, useCallback, useMemo, useState, type ReactNode } from 'react';
import {
  Blocks,
  BookOpen,
  Fingerprint,
  Globe,
  KeyRound,
  LayoutDashboard,
  Layers,
  Lock,
  Package,
  RotateCw,
  Settings as SettingsIcon,
  ShieldCheck,
  Waypoints,
} from 'lucide-react';

import { api, type Progress, type Risk, type StepState } from './lib/api';
import { NavCtx, useNav, type PageId } from './lib/nav';
import { R } from './lib/resources';
import { put, useResource } from './lib/store';
import type { StepId } from './lib/steps';
import { Button, ToastProvider } from './ui';
import { fmtDaysLeft } from './ui/labels';

import Home from './pages/Home';
import Purity from './pages/Purity';
import DnsLeak from './pages/DnsLeak';
import ChineseSignals from './pages/ChineseSignals';
import IpLock from './pages/IpLock';
import Accounts from './pages/Accounts';
import Environment from './pages/Environment';
import SubscriptionGuidePage from './pages/SubscriptionGuidePage';
import Plugins from './pages/Plugins';
import Relay from './pages/Relay';
import Profiles from './pages/Profiles';
import Settings from './pages/Settings';
import AccountDialogs from './pages/accounts/AccountDialogs';

const EMPTY_PROGRESS: Progress = { steps: {}, completed_once: false };

/** 侧栏右侧那条摘要的语气。 */
type SummaryTone = '' | 'ok' | 'warn' | 'danger';

interface NavEntry {
  id: PageId;
  label: string;
  icon: ReactNode;
}

interface NavGroup {
  title?: string;
  items: NavEntry[];
}

/**
 * 侧栏改成按功能分组，不再是 ①②③④⑤ 的向导。
 *
 * 旧侧栏把「按顺序往下走」写死在结构里：五个步骤带序号，走完才换成状态灯。
 * 可实际用法根本不是线性的 —— 每天开面板是来「看一眼状态、点一下启动」的，
 * 不是来重走一遍五步向导的。
 */
const GROUPS: NavGroup[] = [
  {
    items: [{ id: 'home', label: '总览', icon: <LayoutDashboard size={15} /> }],
  },
  {
    title: '安全检查',
    items: [
      { id: 'purity', label: 'IP 纯净度', icon: <ShieldCheck size={15} /> },
      { id: 'dns', label: 'DNS 泄露', icon: <Globe size={15} /> },
      { id: 'signals', label: '中文环境识别', icon: <Fingerprint size={15} /> },
    ],
  },
  {
    title: '门禁与账户',
    items: [
      { id: 'iplock', label: 'IP 锁', icon: <Lock size={15} /> },
      { id: 'accounts', label: '账户与启动', icon: <KeyRound size={15} /> },
    ],
  },
  {
    title: '其它',
    items: [
      { id: 'environment', label: '环境与安装', icon: <Package size={15} /> },
      { id: 'subscription-guide', label: '订阅引导', icon: <BookOpen size={15} /> },
      { id: 'plugins', label: '插件商店', icon: <Blocks size={15} /> },
      { id: 'relay', label: '中转站', icon: <Waypoints size={15} /> },
      { id: 'profiles', label: '档案与快照', icon: <Layers size={15} /> },
      { id: 'settings', label: '设置', icon: <SettingsIcon size={15} /> },
    ],
  },
];

/**
 * 侧栏每一行右侧的实时摘要。
 *
 * 比旧版那个静态「风险度 pill」信息量大得多：那个只有四种词
 * （未检测／通过／注意／高危），而这里直接告诉你「4/4 已锁」「剩 18 天」。
 */
function useSummaries(progress: Progress): Partial<Record<PageId, [string, SummaryTone]>> {
  const gate = useResource('gate', R.gate);
  const dns = useResource('dns', R.dns);
  const signals = useResource('signals', R.signals);
  const accounts = useResource('accounts', R.accounts);
  const software = useResource('software', R.software);
  const plugins = useResource('plugins', R.plugins);

  return useMemo(() => {
    const out: Partial<Record<PageId, [string, SummaryTone]>> = {};

    // 纯净度以**人工复核**为准，所以这里读进度而不是面板自测的结果。
    const p = progress.steps['purity'];
    if (p?.state === 'skipped') out.purity = ['已跳过', 'warn'];
    else if (p?.risk === 'low') out.purity = ['通过', 'ok'];
    else if (p?.risk === 'high') out.purity = ['不合格', 'danger'];
    else if (p?.risk === 'medium') out.purity = ['注意', 'warn'];
    else out.purity = ['未复核', ''];

    if (dns.data) {
      out.dns = dns.data.passed
        ? ['通过', 'ok']
        : [`${dns.data.findings.length} 项问题`, 'danger'];
    } else out.dns = ['未检测', ''];

    if (signals.data) {
      const b = signals.data.band;
      out.signals = [
        `${signals.data.total}/100`,
        b === 'low' ? 'ok' : b === 'medium' ? 'warn' : 'danger',
      ];
    } else out.signals = ['未检测', ''];

    if (gate.data) {
      const g = gate.data;
      const locked = g.targets.filter((t) => t.locked).length;
      if (g.allowlist.length === 0) out.iplock = ['白名单空', 'warn'];
      else if (g.lease.holder) out.iplock = ['已放行', 'warn'];
      else out.iplock = [`${locked}/${g.targets.length} 已锁`, locked === g.targets.length ? 'ok' : 'warn'];
    } else out.iplock = ['—', ''];

    const active = accounts.data?.slots.find((s) => s.active);
    if (active) {
      if (!active.logged_in) out.accounts = ['未登录', 'danger'];
      else {
        const d = active.cli_days_left;
        out.accounts = [
          fmtDaysLeft(d),
          d === null || d === undefined ? '' : d < 0 ? 'danger' : d < 5 ? 'warn' : 'ok',
        ];
      }
    } else if (accounts.data) out.accounts = ['无槽位', 'warn'];

    if (software.data) {
      const cc = software.data.claudeCode.installed;
      const cd = software.data.claudeDesktop.installed;
      out.environment = cc && cd ? ['都已装', 'ok'] : cc || cd ? ['缺一个', 'warn'] : ['未安装', 'warn'];
    }

    const st = plugins.data?.[0];
    if (st) {
      out.plugins = [
        st.state === 'running' ? '运行中' : st.state === 'ready' ? '就绪' : '依赖不齐',
        st.state === 'running' || st.state === 'ready' ? 'ok' : '',
      ];
    }

    return out;
  }, [progress, gate.data, dns.data, signals.data, accounts.data, software.data, plugins.data]);
}

function Sidebar() {
  const { page, go, progress } = useNav();
  const summaries = useSummaries(progress);

  const pending = Object.keys(progress.steps).length;
  const skipped = Object.values(progress.steps).filter((s) => s.state === 'skipped').length;

  return (
    <nav className="sidebar" aria-label="主导航">
      <div className="brand">
        <Lock size={16} className="flex-shrink-0 text-[var(--accent)]" aria-hidden="true" />
        <span className="brand-text">
          <span className="brand-title">ClaudeGate</span>
          <small className="brand-sub">
            {progress.completed_once
              ? skipped > 0
                ? `已走完一轮 · 跳过 ${skipped} 项`
                : '已走完一轮'
              : `检查进度 ${pending} / 5`}
          </small>
        </span>
      </div>

      {GROUPS.map((g, gi) => (
        <div className="navgroup" key={g.title ?? gi}>
          {g.title && <div className="navgroup-title">{g.title}</div>}
          {g.items.map((it) => {
            const s = summaries[it.id];
            return (
              <button
                key={it.id}
                type="button"
                className="navitem"
                aria-current={page === it.id ? 'page' : undefined}
                onClick={() => go(it.id)}
                title={s ? `${it.label} · ${s[0]}` : it.label}
              >
                <span className="navitem-icon" aria-hidden="true">
                  {it.icon}
                </span>
                <span className="navitem-label">{it.label}</span>
                {s && <span className={`navitem-summary ${s[1]}`}>{s[0]}</span>}
              </button>
            );
          })}
        </div>
      ))}
    </nav>
  );
}

/**
 * 出错就整页白屏是最糟的结果 —— 这个面板管着执行锁，白屏意味着用户既看不到
 * 状态也点不到「应急解锁」。所以兜一层，至少把错误显示出来并留一个重载入口。
 */
class ErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <div className="crash">
        <h1>界面出错了</h1>
        <p className="sub mt-2">
          面板的界面崩了，但<strong>执行锁与看门狗都在 Rust 那边，不受影响</strong>。
        </p>
        <pre className="logview mt-4 text-left">{this.state.error.message}</pre>
        <div className="mt-4 flex justify-center gap-2">
          <Button
            variant="primary"
            icon={<RotateCw size={13} />}
            onClick={() => window.location.reload()}
          >
            重新载入界面
          </Button>
        </div>
      </div>
    );
  }
}

function Pages() {
  const { page } = useNav();
  switch (page) {
    case 'home':
      return <Home />;
    case 'purity':
      return <Purity />;
    case 'dns':
      return <DnsLeak />;
    case 'signals':
      return <ChineseSignals />;
    case 'iplock':
      return <IpLock />;
    case 'accounts':
      return <Accounts />;
    case 'environment':
      return <Environment />;
    case 'subscription-guide':
      return <SubscriptionGuidePage />;
    case 'plugins':
      return <Plugins />;
    case 'relay':
      return <Relay />;
    case 'profiles':
      return <Profiles />;
    case 'settings':
      return <Settings />;
  }
}

export default function App() {
  const [page, setPageRaw] = useState<PageId>('home');
  // 切页时把主区域滚回顶部 —— 否则从长页面切到短页面会停在半空。
  const setPage = useCallback((next: PageId) => {
    setPageRaw(next);
    document.querySelector('.main')?.scrollTo({ top: 0 });
  }, []);

  const progressRes = useResource('progress', R.progress);
  const progress = progressRes.data ?? EMPTY_PROGRESS;

  const mark = useCallback(
    async (id: StepId, state: StepState, risk: Risk, detail: string) => {
      // progress_set 返回全量 Progress，直接替换缓存，不做本地乐观更新。
      const next = await api.progressSet(id, state, risk, detail);
      put('progress', next);
    },
    []
  );

  const nav = useMemo(
    () => ({ page, go: setPage, progress, mark }),
    [page, setPage, progress, mark]
  );

  return (
    <ToastProvider>
      <NavCtx.Provider value={nav}>
        <div className="app">
          <Sidebar />
          <main className="main">
            {/* 订阅引导是外来模块，自带一套按 1460px 设计的三栏布局（步骤导航 ·
                正文 · 手机示意图），也自带内外边距。套在 980px 的 `.main-inner`
                里会被压成一条窄缝 —— 它的断点看的是**视口宽度**不是容器宽度，
                窗口开得越大反而挤得越狠。所以这一页单独放宽，边距交给模块自己。 */}
            <div className={page === 'subscription-guide' ? 'main-inner main-inner--wide' : 'main-inner'}>
              <ErrorBoundary key={page}>
                <Pages />
              </ErrorBoundary>
            </div>
          </main>
        </div>
        {/* 账户切换 / 新建槽位的对话框全局只挂一份：总览、账户页都往这里喊。
            挂在页面外面，切页不会把一个正在进行的切换对话框卸掉。 */}
        <ErrorBoundary>
          <AccountDialogs />
        </ErrorBoundary>
      </NavCtx.Provider>
    </ToastProvider>
  );
}
