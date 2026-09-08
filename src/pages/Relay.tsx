import { useEffect, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { api, type Provider } from '../lib/api';
import { QUICKSTART_DOC } from '../prompts';

/** 常见预设。用户可以改成任何中转站。 */
const PRESETS: Array<Omit<Provider, 'id'> & { id: string }> = [
  { id: 'sulianyan', name: '速联言', base_url: 'https://api.sulianyan.com/v1', model: null },
  { id: 'openai', name: 'OpenAI 官方', base_url: 'https://api.openai.com/v1', model: null },
];

export default function Relay() {
  const [cur, setCur] = useState<Provider | null>(null);
  const [form, setForm] = useState<Provider & { api_key: string }>({
    id: 'sulianyan',
    name: '速联言',
    base_url: 'https://api.sulianyan.com/v1',
    model: '',
    api_key: '',
  });
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');
  const [msg, setMsg] = useState('');

  async function refresh() {
    try {
      const p = await api.relayCurrent();
      setCur(p);
      if (p) setForm((f) => ({ ...f, ...p, model: p.model ?? '', api_key: '' }));
    } catch {
      /* 还没有配置过就是空的 */
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function save() {
    setBusy(true);
    setErr('');
    setMsg('');
    try {
      await api.relayApply({
        id: form.id.trim(),
        name: form.name.trim(),
        base_url: form.base_url.trim(),
        model: form.model?.trim() || undefined,
        api_key: form.api_key.trim() || undefined,
      });
      setMsg('已写入 ~/.codex/config.toml（原有配置项保留，并已自动备份）');
      setForm((f) => ({ ...f, api_key: '' }));
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <>
      <h1>中转站</h1>
      <p className="sub">
        给 Codex 配置模型供应商，供「DNS 泄露 · 高级通过」使用。能力对标 cc-switch。
      </p>

      {err && <div className="err">{err}</div>}
      {msg && <div className="notice">{msg}</div>}

      <h2>当前配置</h2>
      <div className="card">
        {cur ? (
          <>
            <div className="row">
              <span>供应商</span>
              <span className="v">
                {cur.name} <span className="pill ok">当前</span>
              </span>
            </div>
            <div className="row">
              <span>Base URL</span>
              <span className="v mono">{cur.base_url}</span>
            </div>
            <div className="row">
              <span>模型</span>
              <span className="v">{cur.model ?? '（未指定）'}</span>
            </div>
          </>
        ) : (
          <div className="empty">Codex 还没有配置过供应商。</div>
        )}
      </div>

      <h2>预设</h2>
      <div className="card">
        {PRESETS.map((p) => (
          <div className="row" key={p.id}>
            <span>
              {p.name}
              <span className="notice" style={{ marginLeft: 8 }}>{p.base_url}</span>
            </span>
            <button
              className="btn"
              onClick={() => setForm((f) => ({ ...f, id: p.id, name: p.name, base_url: p.base_url }))}
            >
              填入
            </button>
          </div>
        ))}
      </div>

      <h2>编辑</h2>
      <div className="card">
        <div className="grid2">
          <label>
            <span className="k">标识 id</span>
            <input
              type="text"
              value={form.id}
              onChange={(e) => setForm({ ...form, id: e.target.value })}
              placeholder="sulianyan"
            />
          </label>
          <label>
            <span className="k">显示名</span>
            <input
              type="text"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
            />
          </label>
        </div>
        <label style={{ display: 'block', marginTop: 9 }}>
          <span className="k">Base URL</span>
          <input
            type="text"
            value={form.base_url}
            onChange={(e) => setForm({ ...form, base_url: e.target.value })}
            placeholder="https://api.example.com/v1"
          />
        </label>
        <div className="grid2" style={{ marginTop: 9 }}>
          <label>
            <span className="k">默认模型（可空）</span>
            <input
              type="text"
              value={form.model ?? ''}
              onChange={(e) => setForm({ ...form, model: e.target.value })}
            />
          </label>
          <label>
            <span className="k">API Key</span>
            <input
              type="password"
              value={form.api_key}
              onChange={(e) => setForm({ ...form, api_key: e.target.value })}
              placeholder="留空则不改动现有 Key"
            />
          </label>
        </div>

        <p className="notice">
          Key 会写进 Codex 自己的 <code>~/.codex/auth.json</code>（那是 Codex 的要求），
          面板本身不保存明文，读回来时也不会把完整 Key 传回界面。
          写入前会自动备份原文件。
        </p>

        <button className="btn primary" onClick={save} disabled={busy || !form.base_url.trim()}>
          {busy ? '写入中…' : '保存并切换'}
        </button>
        <button className="btn" onClick={() => openUrl(QUICKSTART_DOC)}>
          新手指引文档 ↗
        </button>
      </div>

      <div className="card warn">
        <p className="notice" style={{ color: 'var(--warn)', marginTop: 0 }}>
          走中转端点时请留意：据第三方逆向分析主张（未经证实），
          Claude Code 在 <code>ANTHROPIC_BASE_URL</code> 指向中转端点时
          会读取系统时区与中转 hostname。官方 OAuth 直连路径不在该描述范围内。
        </p>
      </div>
    </>
  );
}
