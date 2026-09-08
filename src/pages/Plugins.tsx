import { useEffect, useState, createElement } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { api, type PluginStatus } from '../lib/api';
import { PLUGINS, STATE_CLASS, STATE_LABEL } from '../plugins/registry';

export default function Plugins() {
  const [status, setStatus] = useState<PluginStatus[]>([]);
  const [open, setOpen] = useState<string | null>('sillytavern');
  const [err, setErr] = useState('');

  async function refresh() {
    setErr('');
    try {
      setStatus(await api.pluginList());
    } catch (e) {
      setErr(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    const t = setInterval(refresh, 20000);
    return () => clearInterval(t);
  }, []);

  return (
    <>
      <h1>插件商店</h1>
      <p className="sub">
        把外部工具接进 ClaudeGate，共用同一套 IP 门禁与看门狗。
      </p>

      {err && <div className="err">{err}</div>}

      {PLUGINS.map((meta) => {
        const st = status.find((s) => s.id === meta.id);
        const isOpen = open === meta.id;
        return (
          <div className="card" key={meta.id}>
            <div
              className="row"
              style={{ cursor: 'pointer', borderBottom: 'none' }}
              onClick={() => setOpen(isOpen ? null : meta.id)}
            >
              <span>
                <span className="v">
                  {isOpen ? '▾' : '▸'} {meta.name}
                </span>
                {st && (
                  <span className={`pill ${STATE_CLASS[st.state]}`}>
                    {STATE_LABEL[st.state]}
                  </span>
                )}
              </span>
              <span className="notice">{st?.detail ?? '检测中…'}</span>
            </div>

            <p className="notice" style={{ marginTop: 4 }}>
              {meta.blurb}
            </p>

            {meta.upstream && (
              <p className="notice">
                上游：
                <a
                  href="#"
                  onClick={(e) => {
                    e.preventDefault();
                    void openUrl(meta.upstream!.url);
                  }}
                >
                  {meta.upstream.label}
                </a>{' '}
                · {meta.upstream.license}
                {meta.upstream.license.startsWith('AGPL') && (
                  <>
                    {' '}
                    <span className="pill warn">AGPL</span>
                    ——本面板只是启动与管理它，没有修改其源码；
                    如果你改了酒馆并对外提供服务，需要公开修改后的源码。
                  </>
                )}
              </p>
            )}

            {isOpen && (
              <>
                {st && (
                  <div style={{ marginBottom: 10 }}>
                    {st.checks.map((c) => (
                      <div className="row" key={c.label}>
                        <span>
                          {c.label}
                          {c.ok ? (
                            <span className="pill ok">就绪</span>
                          ) : (
                            <span className="pill bad">缺</span>
                          )}
                        </span>
                        <span className="notice mono" style={{ fontSize: 11 }}>
                          {c.detail}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
                {createElement(meta.panel)}
              </>
            )}
          </div>
        );
      })}

      <div className="card">
        <p className="notice" style={{ marginTop: 0 }}>
          目前只有一个插件。清单格式已经按「日后从远程 index 拉」设计，
          加新插件只要往 <code>src/plugins/registry.ts</code> 里加一条，
          再在 Rust 侧实现检测与启停即可。
        </p>
      </div>
    </>
  );
}
