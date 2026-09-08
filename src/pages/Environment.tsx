import { useEffect, useState } from 'react';
import { api, type Risk, type SoftwareReport } from '../lib/api';
import type { StepApi } from '../App';
import StepFooter from '../components/StepFooter';
import { runScan, type ScanResult } from '../lib/signals';
import { BROWSER_REINSTALL_PROMPT, CLEAN_REINSTALL_PROMPT, type PromptDef } from '../prompts';

type EverLoggedIn = 'unset' | 'yes' | 'no';

function PromptBlock({ p, onClose }: { p: PromptDef; onClose: () => void }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="card accent">
      <div className="v">{p.title}</div>
      <p className="notice">{p.modelHint}</p>
      <button
        className="btn primary"
        onClick={async () => {
          await navigator.clipboard.writeText(p.body);
          setCopied(true);
          setTimeout(() => setCopied(false), 2000);
        }}
      >
        {copied ? '已复制' : '复制提示词'}
      </button>
      <button className="btn" onClick={onClose}>
        收起
      </button>
      <textarea readOnly rows={14} value={p.body} style={{ marginTop: 10 }} />
    </div>
  );
}

export default function Environment(step: StepApi) {
  const [sw, setSw] = useState<SoftwareReport | null>(null);
  const [scan, setScan] = useState<ScanResult | null>(null);
  const [sysTz, setSysTz] = useState<string>('');
  const [ipTz, setIpTz] = useState<string>('');
  const [restore, setRestore] = useState(false);
  const [everLoggedIn, setEverLoggedIn] = useState<EverLoggedIn>('unset');
  const [prompt, setPrompt] = useState<PromptDef | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');

  async function refresh() {
    setBusy(true);
    setErr('');
    try {
      const [s, sc] = await Promise.all([api.detectSoftware(), runScan()]);
      setSw(s);
      setScan(sc);
      try {
        setSysTz(await api.tzCurrent());
      } catch {
        /* 非 Windows 或读不到，留空即可 */
      }
      try {
        setIpTz((await api.probeIp()).timezone ?? '');
      } catch {
        /* 网络不通就不比对 */
      }
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  const anyInstalled = !!(sw?.claudeCode.installed || sw?.claudeDesktop.installed);
  const browserTz = Intl.DateTimeFormat().resolvedOptions().timeZone;
  const tzMismatch = !!ipTz && !!browserTz && ipTz !== browserTz;

  const risk: Risk = !scan
    ? 'unknown'
    : scan.band === 'high'
      ? 'high'
      : scan.band === 'medium' || tzMismatch || !sw?.claudeCode.installed
        ? 'medium'
        : 'low';

  async function applyTz() {
    if (!ipTz) return;
    setErr('');
    try {
      await api.tzApply(ipTz, restore);
      await refresh();
    } catch (e) {
      setErr(String(e));
    }
  }

  return (
    <>
      <h1>第 2 步 · 环境与安装</h1>
      <p className="sub">检测本机是否已装 Claude、浏览器语言时区是否与出口 IP 一致。</p>

      {err && <div className="err">{err}</div>}

      <h2>本机软件</h2>
      <div className="grid4">
        {[sw?.claudeDesktop, sw?.claudeCode, sw?.codex].map(
          (s) =>
            s && (
              <div className="metric" key={s.id}>
                <span className="k">{s.name}</span>
                <span className="v">
                  {s.installed ? (s.version ?? '已安装') : '未安装'}
                  {s.installed ? (
                    <span className="pill ok">已装</span>
                  ) : (
                    <span className="pill neutral">缺</span>
                  )}
                </span>
              </div>
            )
        )}
        {sw?.browsers
          .filter((b) => b.installed)
          .slice(0, 1)
          .map((b) => (
            <div className="metric" key={b.id}>
              <span className="k">浏览器</span>
              <span className="v">
                {b.name}
                {tzMismatch && <span className="pill warn">时区不符</span>}
              </span>
            </div>
          ))}
      </div>

      {anyInstalled ? (
        <div className="card warn" style={{ marginTop: 10 }}>
          <div className="v" style={{ color: 'var(--warn)' }}>已检测到 Claude 相关软件</div>
          <p className="notice" style={{ color: 'var(--warn)' }}>
            如果当前安装有问题（配置写坏、装了一半、要转交机器），
            建议做一次完整卸载重装。这一步可以强制跳过。
          </p>
          <button className="btn danger" onClick={() => setPrompt(CLEAN_REINSTALL_PROMPT)}>
            交给 Codex 完整卸载重装
          </button>
        </div>
      ) : (
        <div className="card" style={{ marginTop: 10 }}>
          <div className="v">未检测到 Claude 相关软件</div>
          <p className="notice">你以前在这台机器上登录过 Claude 吗？</p>
          <button
            className={`btn${everLoggedIn === 'yes' ? ' primary' : ''}`}
            onClick={() => setEverLoggedIn('yes')}
          >
            登录过
          </button>
          <button
            className={`btn${everLoggedIn === 'no' ? ' primary' : ''}`}
            onClick={() => setEverLoggedIn('no')}
          >
            没有
          </button>

          {everLoggedIn === 'yes' && (
            <div className="card warn" style={{ marginTop: 10 }}>
              <p className="notice" style={{ color: 'var(--warn)', marginTop: 0 }}>
                浏览器里可能还留着旧的登录态与站点数据。建议重装浏览器再登录。
              </p>
              <button className="btn" onClick={() => setPrompt(BROWSER_REINSTALL_PROMPT)}>
                浏览器重装提示词
              </button>
            </div>
          )}

          <div style={{ marginTop: 10 }}>
            <button className="btn primary" disabled>
              安装 Claude Code
            </button>
            <button className="btn primary" disabled>
              安装 Claude 桌面端
            </button>
            <p className="notice">
              安装按钮需要先在 <code>installers.lock.json</code> 里钉定官方下载地址与
              SHA-256。哈希为空时程序会拒绝下载安装 —— 这是有意的，
              一个能被绕过的校验比没有校验更危险。
            </p>
          </div>
        </div>
      )}

      {prompt && <PromptBlock p={prompt} onClose={() => setPrompt(null)} />}

      <h2>时区对齐</h2>
      <div className="card">
        <div className="grid3">
          <div className="metric">
            <span className="k">出口 IP 时区</span>
            <span className="v">{ipTz || '—'}</span>
          </div>
          <div className="metric">
            <span className="k">当前系统时区</span>
            <span className="v">{sysTz || '—'}</span>
          </div>
          <div className="metric">
            <span className="k">浏览器读到的</span>
            <span className="v">
              {browserTz}
              {tzMismatch ? (
                <span className="pill bad">不一致</span>
              ) : (
                <span className="pill ok">一致</span>
              )}
            </span>
          </div>
        </div>
        <label className="notice" style={{ display: 'block', marginTop: 8 }}>
          <input
            type="checkbox"
            checked={restore}
            onChange={(e) => setRestore(e.target.checked)}
            style={{ width: 'auto', marginRight: 6 }}
          />
          退出面板时还原为原时区
          <span className="pill neutral">不勾选 · 建议</span>
        </label>
        <p className="notice">
          专机长期跑 Claude 时，时区保持一致比来回切更稳，所以默认不还原。
          切换系统时区需要管理员权限，会弹 UAC。
        </p>
        <button className="btn primary" onClick={applyTz} disabled={!ipTz || !tzMismatch}>
          {tzMismatch ? `切换到 ${ipTz}` : '已经一致，无需切换'}
        </button>
      </div>

      <h2>
        中文环境识别 {scan && <span className={`pill ${scan.band === 'low' ? 'ok' : scan.band === 'medium' ? 'warn' : 'bad'}`}>
          {scan.total} / 100
        </span>}
      </h2>
      <div className="card">
        {scan ? (
          <>
            <div className="bar" style={{ height: 7 }}>
              <i
                style={{
                  width: `${scan.total}%`,
                  background:
                    scan.band === 'low'
                      ? 'var(--ok)'
                      : scan.band === 'medium'
                        ? 'var(--warn)'
                        : 'var(--danger)',
                }}
              />
            </div>
            <div style={{ marginTop: 8 }}>
              {scan.signals.map((s) => (
                <div className="row" key={s.id} title={s.hint}>
                  <span>
                    {s.label}
                    <span className="notice" style={{ marginLeft: 6 }}>权重 {s.weight}</span>
                    {s.claudeUsed && <span className="pill neutral">Claude 会读</span>}
                    {!s.fixable && s.points > 0 && (
                      <span className="pill warn">不可修复</span>
                    )}
                  </span>
                  <span>
                    <span className="notice">{s.raw}</span>
                    <span
                      className={`pill ${s.verdict === 'low' ? 'ok' : s.verdict === 'medium' ? 'warn' : 'bad'}`}
                    >
                      +{s.points}
                    </span>
                  </span>
                </div>
              ))}
            </div>
            <p className="notice">
              分档：低 0–30、中 31–60、高 61–100；单项 score ≥ 0.25 计为命中。
              检测全部在本地完成，不上传任何数据。
              台湾（Asia/Taipei、zh-TW）是 Anthropic 完整支持的地区，故意不计分。
            </p>
            <p className="notice">
              评分逻辑改编自 <code>FuckClaude</code>（MIT），详见 ATTRIBUTION.md。
              其中「Claude Code 会把时区隐写进 system prompt」是第三方逆向主张、
              未经证实，且据其描述只在 <code>ANTHROPIC_BASE_URL</code> 指向中转端点时触发。
            </p>
          </>
        ) : (
          <div className="empty">检测中…</div>
        )}
        <button className="btn" onClick={refresh} disabled={busy}>
          {busy ? '检测中…' : '重新检测'}
        </button>
      </div>

      <StepFooter
        {...step}
        step="environment"
        risk={risk}
        detail={
          scan
            ? `中文环境 ${scan.total}/100，命中 ${scan.hits.length} 项${tzMismatch ? '；时区与 IP 不一致' : ''}`
            : '尚未检测'
        }
        canAdvance={scan !== null}
      />
    </>
  );
}
