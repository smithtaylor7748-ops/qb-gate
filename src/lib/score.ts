/**
 * 综合评分。
 *
 * # 为什么需要归一化
 *
 * 项目里三套分数**方向是打架的**，直接相加会得出完全错误的结论：
 *
 * | 来源 | 范围 | 方向 |
 * |---|---|---|
 * | `IpInfo.fraudScore` | 0–100 | **低了好**（≤5 才算通过） |
 * | `ScanResult.total`（中文环境） | 0–100 | **低了好**（0–30 是低风险） |
 * | `DnsReport.score` | 0–100 | **高了好** |
 *
 * 所以每一项都先折算成「这一项拿到了自己权重的百分之多少」，再加权求和。
 *
 * # 两条不能做错的
 *
 * 1. **分母只算已检测项。** `dns` 和 `signals` 是 `auto: false`，不点不跑。
 *    把没跑过的算成 0 分，面板会在用户什么都没做时就报「环境很差」；
 *    算成满分则更糟 —— 那是在替一个没做过的检测打包票。
 *    未检测的项**从分子分母里一起去掉**，并在界面上明说少了几项。
 *
 * 2. **纯净度取人工判定，不取 `probe_purity`。**
 *    `verdict.rs` 把 `native` 写死成 `Unknown`（公开接口就是没这个字段），
 *    所以 `PanelVerdict.passed` 结构上永远是 `false`。真值在
 *    `progress.steps['purity']` 里 —— 那是用户自己去 IPQS / ippure 核对后
 *    记下的结论。
 */

import type { GateStatus, DnsReport, Progress } from './api';
import { describeLease } from './lease';
import type { ScanResult } from './signals';

export type Band = 'good' | 'fair' | 'poor';

export interface ScoreItem {
  id: 'purity' | 'dns' | 'signals' | 'iplock';
  label: string;
  weight: number;
  /** 得分，0..weight。`null` = 这一项还没检测过，不计入总分。 */
  earned: number | null;
  detail: string;
}

/* 这里原来有个 `page` / `route` 字段，指「点这一格跳哪一页」。
   现在四格点下去是就地开小窗，跳哪儿由 `ScoreBand` 的 `OPENS` 表说了算 ——
   评分算法不该知道界面长什么样。 */

export interface Score {
  /** 0–100。`null` = 一项都没检测过，界面上显示「—」而不是 0。 */
  total: number | null;
  band: Band;
  items: ScoreItem[];
  assessed: number;
  missing: number;
}

/** 权重表。加起来正好 100。 */
const WEIGHTS = {
  purity: 35,
  dns: 25,
  signals: 20,
  iplock: 20,
} as const;

export const BAND_LABEL: Record<Band, string> = {
  good: '良好',
  fair: '尚可',
  poor: '偏差',
};

export const BAND_TONE: Record<Band, 'ok' | 'warn' | 'danger'> = {
  good: 'ok',
  fair: 'warn',
  poor: 'danger',
};

export function bandOf(total: number | null): Band {
  if (total === null) return 'fair';
  if (total >= 85) return 'good';
  if (total >= 60) return 'fair';
  return 'poor';
}

interface Inputs {
  progress: Progress;
  dns?: DnsReport;
  signals?: ScanResult;
  gate?: GateStatus;
}

export function computeScore({ progress, dns, signals, gate }: Inputs): Score {
  const items: ScoreItem[] = [
    purityItem(progress),
    dnsItem(dns),
    signalsItem(signals),
    lockItem(gate),
  ];

  const done = items.filter((i) => i.earned !== null);
  const earned = done.reduce((a, i) => a + (i.earned ?? 0), 0);
  const possible = done.reduce((a, i) => a + i.weight, 0);

  // 一项都没检测过就不给分数。给 0 分是在冤枉用户，给满分是在替没做过的
  // 检测打包票，两个都不对。
  const total = possible === 0 ? null : Math.round((earned / possible) * 100);

  return {
    total,
    band: bandOf(total),
    items,
    assessed: done.length,
    missing: items.length - done.length,
  };
}

// ------------------------------------------------------------ 逐项

function purityItem(progress: Progress): ScoreItem {
  const rec = progress.steps['purity'];
  const base = { id: 'purity' as const, label: 'IP 纯净度', weight: WEIGHTS.purity };

  // 真值在人工判定里 —— probe_purity 的 passed 结构上永远是 false。
  if (!rec || rec.state === 'pending') {
    return { ...base, earned: null, detail: '还没复核过' };
  }
  if (rec.state === 'skipped') {
    return { ...base, earned: null, detail: '已跳过，不计入总分' };
  }
  if (rec.state === 'failed' || rec.risk === 'high') {
    return { ...base, earned: 0, detail: rec.detail || '不合格' };
  }
  // 「注意」档给八折：过了，但用户自己记了个风险。
  const ratio = rec.risk === 'medium' ? 0.8 : 1;
  return {
    ...base,
    earned: Math.round(base.weight * ratio),
    detail: rec.detail || '已复核通过',
  };
}

function dnsItem(dns?: DnsReport): ScoreItem {
  const base = { id: 'dns' as const, label: 'DNS 泄露', weight: WEIGHTS.dns };
  if (!dns) return { ...base, earned: null, detail: '还没检测过' };
  // DnsReport.score 已经是 100 分制且方向一致（高了好）。
  return {
    ...base,
    earned: Math.round((dns.score / 100) * base.weight),
    detail:
      dns.ethernet_safe === true
        ? `${dns.score} / 100 · 以太网无泄露`
        : `${dns.score} / 100 · ${dns.findings.length} 项问题`,
  };
}

function signalsItem(signals?: ScanResult): ScoreItem {
  const base = {
    id: 'signals' as const,
    label: '中文环境',
    weight: WEIGHTS.signals,
  };
  if (!signals) return { ...base, earned: null, detail: '还没检测过' };
  // 方向相反：total 越低越好，所以取补数。
  return {
    ...base,
    earned: Math.round(((100 - signals.total) / 100) * base.weight),
    detail: `识别度 ${signals.total} / 100 · 命中 ${signals.hits.length} 项`,
  };
}

function lockItem(gate?: GateStatus): ScoreItem {
  const base = { id: 'iplock' as const, label: 'IP 锁', weight: WEIGHTS.iplock };
  if (!gate) return { ...base, earned: null, detail: '还没读到门禁状态' };

  const total = gate.targets.length;
  if (total === 0) {
    return { ...base, earned: null, detail: '没有找到可执行副本' };
  }

  // 租约期内是**故意解锁**的，不该因此扣分 —— 那正是面板放行的结果。
  const lease = describeLease(gate.lease, '已放行给');
  if (lease) {
    return { ...base, earned: base.weight, detail: lease.text };
  }

  const locked = gate.targets.filter((t) => t.locked).length;
  let ratio = locked / total;

  // 残留副本是**没有锁的可绕过入口**，比少锁一个更严重。
  const stale = gate.stale_copies.length;
  if (stale > 0) ratio *= 0.5;

  const bits = [`${locked} / ${total} 已锁`];
  if (stale > 0) bits.push(`${stale} 个残留副本可绕过`);
  if (gate.allowlist.length === 0) bits.push('白名单为空');

  return { ...base, earned: Math.round(base.weight * ratio), detail: bits.join(' · ') };
}
