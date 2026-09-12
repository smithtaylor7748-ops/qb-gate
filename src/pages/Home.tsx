/**
 * 总览。
 *
 * 只做组装：页头 + 通栏评分 + 两个页签 + 通栏日志，中间夹着三处异常态。
 *
 * # 布局为什么是这样（v0.7.0）
 *
 *   1. `ScoreBand` —— 综合评分，**通栏，不进页签**。
 *      IP 纯净度 / DNS / 中文环境 / IP 锁讲的是「这台机器安不安全」，
 *      两侧共用同一套指标，塞进任一页签都是错的。
 *   2. `Claude` / `GPT` 两个页签 —— 各自的账户与启动。
 *      分开的理由见 `GptBand.tsx` 的文件头：原来四个磁贴挤在一个 2×2 宫格里，
 *      而左边的账户槽位只对 Claude 有效。
 *   3. `KillBar` —— **通栏，在页签下面**。它按双重证据收进程，两侧的都收。
 *   4. `LogBand` —— 通栏。日志本来就是两边混着记的。
 *
 * # 三处异常态
 *
 * 只在出事时出现，任何一处都不许缩进上面那行 meta 里：
 *   * **门禁已关闭**（v0.7.0 新增）—— 见下面那段注释，这是最要紧的一条；
 *   * 没走完的检查项横幅；
 *   * IP 不合格爆红卡（档案 §4.2 的硬要求）。
 */

import { useState } from 'react';
import { Activity, DoorOpen, Flag, RotateCw, ShieldAlert } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';

import { api } from '../lib/api';
import { useNav } from '../lib/nav';
import { AFTER, R } from '../lib/resources';
import { invalidate, useResource, useSession } from '../lib/store';
import { STEPS } from '../lib/steps';
import { Button, Card, PageHeader, useToast } from '../ui';

import ScoreBand from './home/ScoreBand';
import AccountBand from './home/AccountBand';
import GptBand from './home/GptBand';
import KillBar from './home/KillBar';
import LogBand from './home/LogBand';

type Side = 'claude' | 'gpt';

const SIDES: Array<{ id: Side; label: string; hint: string }> = [
  { id: 'claude', label: 'Claude', hint: '账户槽位 · Claude Code / 桌面端 / 酒馆' },
  { id: 'gpt', label: 'GPT', hint: 'Codex · 中转站 · 门禁范围' },
];

export default function Home() {
  const { go, progress } = useNav();
  const toast = useToast();

  const ip = useResource('ip', R.ip);
  const gate = useResource('gate', R.gate);
  const criteria = useResource('criteria', R.criteria);
  const dns = useResource('dns', R.dns);
  const signals = useResource('signals', R.signals);

  const [checkup, setCheckup] = useState('');
  const [reopening, setReopening] = useState(false);
  const [bannerOff, setBannerOff] = useSession('home.banner.off', false);
  /** 记在会话里 —— 切到别的页再回来，还停在原来那一边。 */
  const [side, setSide] = useSession<Side>('home.side', 'claude');

  const purityRec = progress.steps['purity'];
  const purityFailed = purityRec?.state === 'failed' || purityRec?.risk === 'high';

  const untouched = STEPS.filter((s) => !progress.steps[s.id]).length;
  const skipped = Object.values(progress.steps).filter((s) => s.state === 'skipped').length;

  /** 门被面板自己关上了，而且还没能自己开回来。 */
  const needsReopen = gate.data?.needs_reopen ?? null;

  /**
   * 一键全面体检。
   *
   * 串行跑，因为 DNS 那项本身就要六秒、中文环境识别里还有个 1 秒的
   * WebRTC 超时 —— 并发跑省不了多少时间，却会让进度文字没法看。
   *
   * **跑不了 IP 纯净度**：那一项的结论只能由用户自己去 IPQS / ippure 看，
   * 面板不替他判定。所以体检完那一项可能仍然是「未检测」，这是对的。
   */
  async function runCheckup() {
    const steps: Array<[string, () => Promise<unknown>]> = [
      ['出口 IP', () => ip.refresh()],
      ['门禁状态', () => gate.refresh()],
      ['DNS 泄露', () => dns.refresh()],
      ['中文环境识别', () => signals.refresh()],
    ];
    for (const [label, run] of steps) {
      setCheckup(label);
      try {
        await run();
      } catch {
        // 单项失败不中断整轮 —— 各自的卡片会显示自己的错误。
      }
    }
    setCheckup('');
    toast.ok('体检跑完了。IP 纯净度需要你自己去权威站点核对。');
  }

  /** 重新放行：验一次出口 IP，过了就把门开回来并接回看门狗。 */
  async function reopen() {
    setReopening(true);
    try {
      toast.ok(await api.gateReopen());
      invalidate(...AFTER.lease);
    } catch (e) {
      // 验不过就该开不了 —— 原文已经说清是「不在白名单」还是「查不到 IP」，
      // 两者必须分开，不能合并成「重新放行失败」。
      toast.error(e instanceof Error ? e.message : String(e));
    } finally {
      setReopening(false);
    }
  }

  return (
    <>
      <PageHeader
        title="总览"
        sub="上面看状态，下面点启动。"
        actions={
          <>
            <Button
              icon={<RotateCw size={13} />}
              loading={ip.loading}
              disabled={!!checkup}
              onClick={() => {
                void ip.refresh();
                void gate.refresh();
              }}
            >
              重新检测
            </Button>
            <Button
              variant="primary"
              icon={<Activity size={13} />}
              loading={!!checkup}
              onClick={() => void runCheckup()}
            >
              {checkup ? `正在查${checkup}…` : '一键全面体检'}
            </Button>
          </>
        }
      />

      <ScoreBand />

      {/* 门禁被面板自己关上了。

          这一条必须常驻、必须显眼，因为它是唯一一个**使用者必须知道、
          却完全看不见**的状态：面板多半收在托盘里，ip-gate.log 等于没写，
          而症状要等他下次在 Claude 桌面端开新会话时才出现
          （`Claude Code couldn't start`）—— 那时候他不会把两件事联系起来。
          不给「不再提醒」，因为它不是提醒，是一个待办。 */}
      {needsReopen && (
        <div className="banner banner--danger mb-3">
          <DoorOpen size={14} className="banner-icon" aria-hidden="true" />
          <div className="banner-body">
            <strong>门禁已关闭</strong>
            <span className="notice block">{needsReopen}</span>
          </div>
          <div className="flex flex-shrink-0 gap-1.5">
            <Button size="sm" variant="primary" loading={reopening} onClick={() => void reopen()}>
              重新放行
            </Button>
            <Button size="sm" variant="ghost" onClick={() => go('iplock')}>
              去 IP 锁
            </Button>
          </div>
        </div>
      )}

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

      {/* ---------------------------------------------- Claude / GPT 页签 */}
      <div className="sidetabs mb-3" role="tablist" aria-label="账户与启动">
        {SIDES.map((s) => (
          <button
            key={s.id}
            type="button"
            role="tab"
            aria-selected={side === s.id}
            className={`sidetab${side === s.id ? ' sidetab--on' : ''}`}
            onClick={() => setSide(s.id)}
          >
            <span className="sidetab-name">{s.label}</span>
            <span className="sidetab-hint">{s.hint}</span>
          </button>
        ))}
      </div>

      {side === 'claude' ? <AccountBand /> : <GptBand />}

      {/* 通栏，在页签外面 —— 它两侧的进程都收。 */}
      <Card className="mb-3">
        <KillBar />
      </Card>

      <LogBand />
    </>
  );
}
