import { useEffect, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { api, type IpInfo, type PanelVerdict, type PurityCriteria, type Risk } from '../lib/api';
import type { StepApi } from '../App';
import StepFooter from '../components/StepFooter';

type Manual = 'unset' | 'pass' | 'fail';

function mark(c: 'Pass' | 'Fail' | 'Unknown') {
  if (c === 'Pass') return <span className="pill ok">通过</span>;
  if (c === 'Fail') return <span className="pill bad">不通过</span>;
  return <span className="pill neutral">未知</span>;
}

export default function Purity(step: StepApi) {
  const [info, setInfo] = useState<IpInfo | null>(null);
  const [verdict, setVerdict] = useState<PanelVerdict | null>(null);
  const [crit, setCrit] = useState<PurityCriteria | null>(null);
  const [manual, setManual] = useState<Manual>('unset');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');

  async function scan() {
    setBusy(true);
    setErr('');
    try {
      const [i, v] = await Promise.all([api.probeIp(), api.probePurity()]);
      setInfo(i);
      setVerdict(v);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void scan();
    api.purityCriteria().then(setCrit).catch(() => undefined);
  }, []);

  // 判定以**人工复核**为准。面板自测只影响提示，不决定通过与否。
  const risk: Risk = manual === 'pass' ? 'low' : manual === 'fail' ? 'high' : 'unknown';
  const detail =
    manual === 'pass'
      ? `已人工确认 IPQS 与 ippure 均通过（${info?.ip ?? '未知 IP'}）`
      : manual === 'fail'
        ? '人工复核未通过：三项硬指标至少缺一'
        : '尚未人工复核';

  return (
    <>
      <h1>第 1 步 · IP 纯净度</h1>
      <p className="sub">
        三项硬指标缺一不可：纯净度 ≤ {crit?.maxFraudScore ?? 5}%、原生 IP、住宅 IP。
        不合格可强制跳过，但会记在总览里。
      </p>

      {manual === 'fail' && (
        <div className="card danger">
          <div className="v" style={{ color: 'var(--danger)' }}>IP 不合格 — 面板已爆红</div>
          <p className="notice" style={{ color: 'var(--danger)' }}>
            继续用这条 IP 登录，风险由你自己承担。
          </p>
          <button className="btn danger" onClick={() => crit && openUrl(crit.iproyal)}>
            前往 IPRoyal 购买住宅 IP ↗
          </button>
        </div>
      )}

      <h2>权威复核 · 以这两家为准</h2>
      <div className="card accent">
        <p className="notice" style={{ marginTop: 0 }}>
          面板不替你判定。请自己打开这两个站点看结果，再回来勾选。
        </p>

        <div className="row">
          <div style={{ minWidth: 0 }}>
            <div className="v">① IPQualityScore</div>
            <div className="notice">{crit?.ipqs.criteria ?? '载入中…'}</div>
          </div>
          <button className="btn primary" onClick={() => crit && openUrl(crit.ipqs.url)}>
            打开 ↗
          </button>
        </div>

        <div className="row">
          <div style={{ minWidth: 0 }}>
            <div className="v">② ippure.com</div>
            <div className="notice">{crit?.ippure.criteria ?? '载入中…'}</div>
          </div>
          <button className="btn primary" onClick={() => crit && openUrl(crit.ippure.url)}>
            打开 ↗
          </button>
        </div>

        <div style={{ marginTop: 12 }}>
          <span className="notice" style={{ marginRight: 10 }}>查完在这里勾：</span>
          <button
            className={`btn${manual === 'pass' ? ' primary' : ''}`}
            onClick={() => setManual('pass')}
          >
            两家均通过
          </button>
          <button
            className={`btn${manual === 'fail' ? ' danger' : ''}`}
            onClick={() => setManual('fail')}
          >
            有不通过
          </button>
        </div>
      </div>

      <h2>
        面板自测 <span className="pill warn">不权威</span>
      </h2>
      <div className="card">
        {err && <div className="err">{err}</div>}
        <div className="grid3">
          <div className="metric">
            <span className="k">IPPure 系数</span>
            <span className="v">
              {info?.fraudScore ?? '—'}
              {verdict && mark(verdict.purity)}
            </span>
            <div className="bar">
              <i
                style={{
                  width: `${Math.min(100, info?.fraudScore ?? 0)}%`,
                  background:
                    (info?.fraudScore ?? 100) <= (crit?.maxFraudScore ?? 5)
                      ? 'var(--ok)'
                      : 'var(--danger)',
                }}
              />
            </div>
          </div>
          <div className="metric">
            <span className="k">IP 属性</span>
            <span className="v">
              {info?.isResidential === true
                ? '住宅 IP'
                : info?.isResidential === false
                  ? '非住宅'
                  : '—'}
              {verdict && mark(verdict.residential)}
            </span>
          </div>
          <div className="metric">
            <span className="k">IP 来源</span>
            <span className="v">
              原生 IP{verdict && mark(verdict.native)}
            </span>
          </div>
        </div>

        <div className="grid3" style={{ marginTop: 9 }}>
          <div className="metric">
            <span className="k">出口 IP</span>
            <span className="v mono">{info?.ip ?? '—'}</span>
          </div>
          <div className="metric">
            <span className="k">ASN / 运营商</span>
            <span className="v">{info?.asOrganization ?? '—'}</span>
          </div>
          <div className="metric">
            <span className="k">位置 / 时区</span>
            <span className="v">
              {info?.countryCode ?? '—'} · {info?.timezone ?? '—'}
            </span>
          </div>
        </div>

        <p className="notice">
          {verdict?.note ??
            '面板自测仅供参考，不权威。以 IPQualityScore 与 ippure.com 的结果为准。'}
          {verdict?.native === 'Unknown' &&
            '「原生 IP」公开接口没有这个字段，面板拿不到，只能去站点上看。'}
        </p>
        <button className="btn" onClick={scan} disabled={busy}>
          {busy ? '检测中…' : '重新检测'}
        </button>
        {crit?.optional.map((o) => (
          <button key={o.name} className="btn" onClick={() => openUrl(o.url)}>
            {o.name} ↗
          </button>
        ))}
      </div>

      <StepFooter
        {...step}
        step="purity"
        risk={risk}
        detail={detail}
        canAdvance={manual !== 'unset'}
      />
    </>
  );
}
