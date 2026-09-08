import { useEffect, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  api,
  type DnsReport,
  type GateStatus,
  type IpInfo,
  type PurityCriteria,
  type Slot,
  type SoftwareReport,
} from '../lib/api';
import type { PageId, StepApi } from '../App';
import { RISK_LABEL, STEPS, stepIndex } from '../lib/steps';
import { runScan, type ScanResult } from '../lib/signals';

export default function Dashboard({ progress, go }: StepApi) {
  const [ip, setIp] = useState<IpInfo | null>(null);
  const [gate, setGate] = useState<GateStatus | null>(null);
  const [sw, setSw] = useState<SoftwareReport | null>(null);
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [dns, setDns] = useState<DnsReport | null>(null);
  const [slots, setSlots] = useState<Slot[]>([]);
  const [crit, setCrit] = useState<PurityCriteria | null>(null);
  const [at, setAt] = useState('');

  useEffect(() => {
    let live = true;
    async function load() {
      // 各查各的，一个失败不影响其他卡片。DNS 那条最慢，单独放后面。
      api.probeIp().then((v) => live && setIp(v)).catch(() => undefined);
      api.gateStatus().then((v) => live && setGate(v)).catch(() => undefined);
      api.detectSoftware().then((v) => live && setSw(v)).catch(() => undefined);
      api.accountsList().then((v) => live && setSlots(v.slots)).catch(() => undefined);
      api.purityCriteria().then((v) => live && setCrit(v)).catch(() => undefined);
      runScan().then((v) => live && setScan(v)).catch(() => undefined);
      setAt(new Date().toLocaleTimeString());
    }
    void load();
    const t = setInterval(load, 60000);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, []);

  const skipped = Object.values(progress.steps).filter((s) => s.state === 'skipped').length;
  const failed = Object.values(progress.steps).filter((s) => s.state === 'failed').length;
  const purityRec = progress.steps['purity'];
  const purityFailed = purityRec?.state === 'failed' || purityRec?.risk === 'high';

  const active = slots.find((s) => s.active);
  const browserTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const tzMismatch = !!ip?.timezone && ip.timezone !== browserTz;

  return (
    <>
      <h1>总览</h1>
      <p className="sub">最后刷新 {at || '—'} · 每 60 秒自动刷新</p>

      {purityFailed ? (
        <div className="card danger">
          <div className="v" style={{ color: 'var(--danger)' }}>IP 不合格 — 面板已爆红</div>
          <p className="notice" style={{ color: 'var(--danger)', marginTop: 3 }}>
            {purityRec?.detail || '三项硬指标至少缺一：纯净度 / 原生 IP / 住宅 IP'}
          </p>
          <button className="btn danger" onClick={() => crit && openUrl(crit.iproyal)}>
            前往 IPRoyal 购买住宅 IP ↗
          </button>
          <button className="btn" onClick={() => go('purity')}>
            重新检测
          </button>
        </div>
      ) : failed + skipped > 0 ? (
        <div className="card warn">
          <div className="v" style={{ color: 'var(--warn)' }}>
            有 {failed + skipped} 项未达标
          </div>
          <p className="notice" style={{ color: 'var(--warn)', marginTop: 3 }}>
            {skipped > 0 && `你强制跳过了 ${skipped} 项。`}
            {failed > 0 && `${failed} 项检测未通过。`}
          </p>
        </div>
      ) : progress.completed_once ? (
        <div className="card ok">
          <div className="v" style={{ color: 'var(--ok)' }}>全部检测项通过</div>
        </div>
      ) : (
        <div className="card accent">
          <div className="v">还没走完一轮检测</div>
          <p className="notice" style={{ marginTop: 3 }}>
            左侧五个菜单就是引导步骤，从「IP 纯净度」开始往下走。随时可以跳着点。
          </p>
          <button className="btn primary" onClick={() => go('purity')}>
            从第 1 步开始 →
          </button>
        </div>
      )}

      <h2>检测项状态</h2>
      <div className="card">
        {STEPS.map((s) => {
          const rec = progress.steps[s.id];
          const risk = rec?.risk ?? 'unknown';
          return (
            <div className="row" key={s.id}>
              <span>
                {stepIndex(s.id)}. {s.label}
                <span className="notice" style={{ marginLeft: 8 }}>
                  {rec?.detail || s.blurb}
                </span>
              </span>
              <span>
                {rec?.state === 'skipped' && <span className="pill warn">已跳过</span>}
                <span
                  className={`pill ${risk === 'low' ? 'ok' : risk === 'medium' ? 'warn' : risk === 'high' ? 'bad' : 'neutral'}`}
                >
                  {RISK_LABEL[risk]}
                </span>
                <button
                  className="btn"
                  style={{ margin: 0, marginLeft: 6 }}
                  onClick={() => go(s.id as PageId)}
                >
                  打开
                </button>
              </span>
            </div>
          );
        })}
      </div>

      <h2>网络出口</h2>
      <div className="grid3">
        <div className="metric">
          <span className="k">出口 IP</span>
          <span className="v mono">{ip?.ip ?? '—'}</span>
        </div>
        <div className="metric">
          <span className="k">ASN / 运营商</span>
          <span className="v">{ip?.asOrganization ?? '—'}</span>
        </div>
        <div className="metric">
          <span className="k">位置 / 时区</span>
          <span className="v">
            {ip?.countryCode ?? '—'} · {ip?.timezone ?? '—'}
            {tzMismatch && <span className="pill warn">系统不符</span>}
          </span>
        </div>
      </div>
      <div className="grid3" style={{ marginTop: 9 }}>
        <div className="metric">
          <span className="k">IPPure 系数</span>
          <span className="v">
            {ip?.fraudScore ?? '—'}
            {ip?.fraudScore !== null && ip?.fraudScore !== undefined && (
              <span className={`pill ${ip.fraudScore <= (crit?.maxFraudScore ?? 5) ? 'ok' : 'bad'}`}>
                {ip.fraudScore <= (crit?.maxFraudScore ?? 5) ? '达标' : '超标'}
              </span>
            )}
          </span>
        </div>
        <div className="metric">
          <span className="k">IP 属性</span>
          <span className="v">
            {ip?.isResidential === true ? '住宅 IP' : ip?.isResidential === false ? '非住宅' : '—'}
          </span>
        </div>
        <div className="metric">
          <span className="k">人工复核</span>
          <span className="v">
            {purityRec?.updated_at ? purityRec.updated_at : '未复核'}
          </span>
        </div>
      </div>

      <h2>泄露与环境</h2>
      <div className="grid3">
        <div className="metric">
          <span className="k">DNS 泄露</span>
          <span className="v">
            {dns ? (dns.passed ? '通过' : `${dns.findings.length} 项问题`) : '未检测'}
            <button
              className="btn"
              style={{ margin: 0, marginLeft: 6, padding: '2px 7px' }}
              onClick={() => api.probeDns().then(setDns).catch(() => undefined)}
            >
              检测
            </button>
          </span>
        </div>
        <div className="metric">
          <span className="k">WebRTC</span>
          <span className="v">
            {scan?.signals.find((s) => s.id === 'webrtcLeak')?.raw ?? '—'}
          </span>
        </div>
        <div className="metric">
          <span className="k">中文环境</span>
          <span className="v">
            {scan ? `${scan.total} / 100` : '—'}
            {scan && (
              <span
                className={`pill ${scan.band === 'low' ? 'ok' : scan.band === 'medium' ? 'warn' : 'bad'}`}
              >
                {scan.band === 'low' ? '低' : scan.band === 'medium' ? '中' : '高'}
              </span>
            )}
          </span>
        </div>
      </div>

      <h2>本机环境</h2>
      <div className="grid4">
        <div className="metric">
          <span className="k">Claude 桌面端</span>
          <span className="v">{sw?.claudeDesktop.version ?? (sw?.claudeDesktop.installed ? '已装' : '未装')}</span>
        </div>
        <div className="metric">
          <span className="k">Claude Code</span>
          <span className="v">{sw?.claudeCode.version ?? (sw?.claudeCode.installed ? '已装' : '未装')}</span>
        </div>
        <div className="metric">
          <span className="k">Codex</span>
          <span className="v">{sw?.codex.installed ? '已装' : '未装'}</span>
        </div>
        <div className="metric">
          <span className="k">浏览器</span>
          <span className="v">
            {sw?.browsers.filter((b) => b.installed).length ?? 0} 个
          </span>
        </div>
      </div>

      <h2>IP 锁</h2>
      <div className="grid4">
        <div className="metric">
          <span className="k">执行锁</span>
          <span className="v">
            {gate ? `${gate.targets.filter((t) => t.locked).length} / ${gate.targets.length} 已锁` : '—'}
          </span>
        </div>
        <div className="metric">
          <span className="k">白名单</span>
          <span className="v">
            {gate?.allowlist.length ?? 0} 条
            {gate?.ip_allowed && <span className="pill ok">命中</span>}
          </span>
        </div>
        <div className="metric">
          <span className="k">看门狗</span>
          <span className="v">{gate?.watchdog_running ? '运行中' : '未运行'}</span>
        </div>
        <div className="metric">
          <span className="k">残留副本</span>
          <span className="v">
            {gate?.stale_copies.length ?? 0}
            {!!gate?.stale_copies.length && <span className="pill bad">可绕过</span>}
          </span>
        </div>
      </div>
      {gate?.recent_log.length ? (
        <div className="card" style={{ marginTop: 9 }}>
          <span className="k">ip-gate.log 最近一条</span>
          <span className="v mono" style={{ fontSize: 11 }}>
            {gate.recent_log[gate.recent_log.length - 1]}
          </span>
        </div>
      ) : null}

      <h2>账户</h2>
      <div className="grid3">
        <div className="metric">
          <span className="k">当前槽位</span>
          <span className="v">{active?.label ?? '无'}</span>
        </div>
        <div className="metric">
          <span className="k">登录态</span>
          <span className="v">{active?.logged_in ? '已登录' : '未登录'}</span>
        </div>
        <div className="metric">
          <span className="k">凭证剩余</span>
          <span className="v">
            {active?.cli_days_left !== null && active?.cli_days_left !== undefined
              ? `${active.cli_days_left} 天`
              : '—'}
          </span>
        </div>
      </div>
      <p className="notice">
        剩余天数只读本地时间戳，查不出「被风控下线」。唯一能确认的办法是实际发一次认证请求。
      </p>

      <div className="card accent" style={{ marginTop: 14 }}>
        <button className="btn primary" onClick={() => go('accounts')}>
          验证 IP 后启动
        </button>
        <button className="btn" onClick={() => go('iplock')}>
          查看 IP 锁
        </button>
      </div>
    </>
  );
}
