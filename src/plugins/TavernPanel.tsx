import { useEffect, useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import {
  api,
  type BackupEntry,
  type CategoryListing,
  type TavernConfig,
} from '../lib/api';

function fmtSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export default function TavernPanel() {
  const [cfg, setCfg] = useState<TavernConfig | null>(null);
  const [assets, setAssets] = useState<CategoryListing[]>([]);
  const [backups, setBackups] = useState<BackupEntry[]>([]);
  const [busy, setBusy] = useState('');
  const [err, setErr] = useState('');
  const [msg, setMsg] = useState('');
  const [openCat, setOpenCat] = useState<string | null>('worlds');
  const [showCfg, setShowCfg] = useState(false);

  async function refresh() {
    setErr('');
    try {
      const [c, a, b] = await Promise.all([
        api.tavernConfig(),
        api.tavernAssets(),
        api.tavernBackups(),
      ]);
      setCfg(c);
      setAssets(a);
      setBackups(b);
    } catch (e) {
      setErr(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function act(label: string, fn: () => Promise<unknown>, ok?: string) {
    setBusy(label);
    setErr('');
    setMsg('');
    try {
      const r = await fn();
      setMsg(ok ?? (typeof r === 'string' ? r : '完成'));
      await refresh();
      return r;
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy('');
    }
  }

  return (
    <>
      {err && <div className="err">{err}</div>}
      {msg && <div className="notice">{msg}</div>}

      <div className="card accent">
        <p className="notice" style={{ marginTop: 0 }}>
          启动会先校验出口 IP，通过后才拉起桥接与酒馆，并挂上 ClaudeGate 的看门狗。
          门禁不过就一个进程都不会起。
        </p>
        <button
          className="btn primary"
          disabled={!!busy}
          onClick={async () => {
            const url = await act('start', api.pluginStart);
            if (typeof url === 'string' && url.startsWith('http')) {
              // 无论是新起的还是复用现有服务，都要把页面打开 ——
              // 少了这一步，成功的启动和崩溃看起来一模一样。
              await openUrl(url);
            }
          }}
        >
          {busy === 'start' ? '启动中…（最长 80 秒）' : '启动酒馆'}
        </button>
        <button
          className="btn danger"
          disabled={!!busy}
          onClick={() => act('stop', api.pluginStop, '酒馆与桥接已停止，租约已收回')}
        >
          {busy === 'stop' ? '停止中…' : '停止'}
        </button>
        <button className="btn" onClick={() => setShowCfg((v) => !v)}>
          {showCfg ? '收起路径设置' : '路径设置'}
        </button>
      </div>

      {showCfg && cfg && (
        <div className="card">
          <p className="notice" style={{ marginTop: 0 }}>
            默认值指向本机现有部署。换机器或换目录时在这里改。
          </p>
          {(
            [
              ['bridge_root', '桥接项目目录'],
              ['sillytavern_root', 'SillyTavern 目录'],
              ['st_launcher', '酒馆启动脚本'],
            ] as const
          ).map(([k, label]) => (
            <label key={k} style={{ display: 'block', marginBottom: 9 }}>
              <span className="k">{label}</span>
              <input
                type="text"
                value={cfg[k]}
                onChange={(e) => setCfg({ ...cfg, [k]: e.target.value })}
              />
            </label>
          ))}
          <div className="grid2">
            <label>
              <span className="k">桥接端口</span>
              <input
                type="text"
                value={cfg.bridge_port}
                onChange={(e) =>
                  setCfg({ ...cfg, bridge_port: Number(e.target.value) || 0 })
                }
              />
            </label>
            <label>
              <span className="k">酒馆端口</span>
              <input
                type="text"
                value={cfg.st_port}
                onChange={(e) => setCfg({ ...cfg, st_port: Number(e.target.value) || 0 })}
              />
            </label>
          </div>
          <button
            className="btn primary"
            style={{ marginTop: 9 }}
            disabled={!!busy}
            onClick={() => act('cfg', () => api.tavernConfigSave(cfg), '路径已保存')}
          >
            保存
          </button>
        </div>
      )}

      <h2>资产</h2>
      <div className="card">
        <p className="notice" style={{ marginTop: 0 }}>
          面板只做盘点与搬运。世界书、角色卡的**编辑仍在酒馆自己界面里**做 ——
          在面板里重写一套编辑器，只会永远落后于上游。
        </p>
        {assets.map((c) => (
          <div key={c.id}>
            <div
              className="row"
              style={{ cursor: 'pointer' }}
              onClick={() => setOpenCat(openCat === c.id ? null : c.id)}
            >
              <span>
                {openCat === c.id ? '▾' : '▸'} {c.label}
                <span className="notice" style={{ marginLeft: 8 }}>
                  {c.exists ? `${c.items.length} 项` : '目录不存在'}
                </span>
              </span>
              <span className="notice mono" style={{ fontSize: 11 }}>
                {c.dir}
              </span>
            </div>
            {openCat === c.id &&
              (c.items.length ? (
                c.items.map((it) => (
                  <div
                    className="row"
                    key={it.path}
                    style={{ paddingLeft: 18, borderBottom: 'none' }}
                  >
                    <span className="notice">
                      {it.is_dir ? '📁 ' : ''}
                      {it.name}
                    </span>
                    <span className="notice">
                      {fmtSize(it.size)} · {it.modified ?? '—'}
                    </span>
                  </div>
                ))
              ) : (
                <div className="empty" style={{ paddingLeft: 18 }}>
                  这一类还是空的
                </div>
              ))}
          </div>
        ))}
      </div>

      <h2>备份</h2>
      <div className="card">
        <p className="notice" style={{ marginTop: 0 }}>
          备份是**目录复制**不是打包 —— 出问题时你可以直接进文件夹翻，
          不需要本程序也能恢复。恢复之前会自动把现状再存一份，点错了能退回去。
        </p>
        <button
          className="btn primary"
          disabled={!!busy}
          onClick={() => act('backup', api.tavernBackup, '已备份')}
        >
          {busy === 'backup' ? '备份中…' : '立即备份全部资产'}
        </button>

        {backups.length ? (
          backups.map((b) => (
            <div className="row" key={b.id}>
              <span>
                {b.id}
                <span className="notice" style={{ marginLeft: 8 }}>
                  {b.created} · {fmtSize(b.size)}
                </span>
              </span>
              <button
                className="btn"
                style={{ margin: 0 }}
                disabled={!!busy}
                onClick={() => act('restore', () => api.tavernRestore(b.id))}
              >
                恢复
              </button>
            </div>
          ))
        ) : (
          <div className="empty">还没有备份。</div>
        )}
      </div>
    </>
  );
}
