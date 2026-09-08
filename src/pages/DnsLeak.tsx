import { useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { api, type DnsReport, type Risk } from '../lib/api';
import type { StepApi } from '../App';
import StepFooter from '../components/StepFooter';
import { ALL_PROMPTS, DNS_LEAK_PROMPT, QUICKSTART_DOC } from '../prompts';

export default function DnsLeak(step: StepApi) {
  const [report, setReport] = useState<DnsReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');
  const [showPrompt, setShowPrompt] = useState(false);
  const [copied, setCopied] = useState(false);

  async function run() {
    setBusy(true);
    setErr('');
    try {
      setReport(await api.probeDns());
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function copyPrompt() {
    await navigator.clipboard.writeText(DNS_LEAK_PROMPT.body);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  const risk: Risk = !report
    ? 'unknown'
    : report.passed
      ? 'low'
      : report.findings.length > 1
        ? 'high'
        : 'medium';

  return (
    <>
      <h1>第 3 步 · DNS 泄露</h1>
      <p className="sub">
        简易通过走真实解析回显 + 网卡配置两层；高级通过交给 Codex 深查并修复。
      </p>

      <h2>简易通过</h2>
      <div className="card">
        <p className="notice" style={{ marginTop: 0 }}>
          向 bash.ws 取一个测试 id，依次解析 10 个探针域名，
          由它的权威域名服务器回报「是谁来查的」，再叠加本机网卡 DNS 配置一起判定。
        </p>

        {err && <div className="err">{err}</div>}

        {report && (
          <>
            <div className="grid3">
              <div className="metric">
                <span className="k">判定</span>
                <span className="v">
                  {report.passed ? (
                    <span className="pill ok">通过</span>
                  ) : (
                    <span className="pill bad">发现 {report.findings.length} 项问题</span>
                  )}
                </span>
              </div>
              <div className="metric">
                <span className="k">解析器数量</span>
                <span className="v">{report.resolvers.length}</span>
              </div>
              <div className="metric">
                <span className="k">出口 ASN</span>
                <span className="v">{report.egress_asn ?? '—'}</span>
              </div>
            </div>

            {report.findings.length > 0 && (
              <div className="card danger" style={{ marginTop: 10 }}>
                {report.findings.map((f) => (
                  <div key={f} className="row" style={{ color: 'var(--danger)' }}>
                    {f}
                  </div>
                ))}
              </div>
            )}

            {report.resolvers.length > 0 && (
              <div style={{ marginTop: 10 }}>
                {report.resolvers.slice(0, 12).map((r) => (
                  <div className="row" key={`${r.address}-${r.interface ?? ''}`}>
                    <span className="v mono">{r.address}</span>
                    <span>
                      {r.country_name ?? r.country_code ?? (r.is_private ? '内网' : '未知')}
                      {r.from_adapter && (
                        <span className="pill neutral">网卡 {r.interface}</span>
                      )}
                      {r.is_domestic && <span className="pill bad">国内</span>}
                    </span>
                  </div>
                ))}
                {report.resolvers.length > 12 && (
                  <p className="notice">另有 {report.resolvers.length - 12} 条未列出</p>
                )}
              </div>
            )}

            {report.upstream_conclusion && (
              <p className="notice">bash.ws 结论：{report.upstream_conclusion}</p>
            )}
            <p className="notice">{report.note}</p>
          </>
        )}

        <button className="btn primary" onClick={run} disabled={busy} style={{ marginTop: 8 }}>
          {busy ? '检测中…（约 6 秒）' : report ? '重新检测' : '开始检测'}
        </button>
      </div>

      <h2>高级通过</h2>
      <div className="card accent">
        <p className="notice" style={{ marginTop: 0 }}>
          让 Codex 读网卡配置、路由表、系统代理、浏览器 DoH，必要时用 PktMon 抓包核实，
          发现问题给出修复方案。<strong>{DNS_LEAK_PROMPT.modelHint}</strong>
        </p>

        <button className="btn" onClick={() => setShowPrompt((v) => !v)}>
          {showPrompt ? '收起提示词' : '查看提示词'}
        </button>
        <button className="btn primary" onClick={copyPrompt}>
          {copied ? '已复制' : '复制提示词'}
        </button>
        <button className="btn" onClick={() => step.go('relay')}>
          配置中转站 / 安装 Codex
        </button>
        <button className="btn" onClick={() => openUrl(QUICKSTART_DOC)}>
          新手指引文档 ↗
        </button>

        {showPrompt && (
          <textarea
            readOnly
            rows={18}
            value={DNS_LEAK_PROMPT.body}
            style={{ marginTop: 10 }}
          />
        )}

        <p className="notice">
          提示词全文另存于 <code>src/prompts/index.ts</code>，
          共 {ALL_PROMPTS.length} 份（DNS 排查、卸载重装、浏览器重装），可自行修改。
        </p>
      </div>

      <StepFooter
        {...step}
        step="dns"
        risk={risk}
        detail={
          report
            ? report.passed
              ? '简易检测通过，未发现配置层或解析层泄露'
              : `发现 ${report.findings.length} 项：${report.findings[0]}`
            : '尚未检测'
        }
        canAdvance={report !== null}
      />
    </>
  );
}
