import { useState } from 'react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { api } from '../lib/api';
import { ALL_PROMPTS, type PromptDef } from '../prompts';

export default function Settings() {
  const [open, setOpen] = useState<PromptDef | null>(null);
  const [msg, setMsg] = useState('');
  const [err, setErr] = useState('');

  async function restoreTz() {
    setErr('');
    setMsg('');
    try {
      await api.tzRestore();
      setMsg('已还原为切换前的系统时区。');
    } catch (e) {
      setErr(String(e));
    }
  }

  return (
    <>
      <h1>设置</h1>
      <p className="sub">提示词、还原操作与合规说明。</p>

      {err && <div className="err">{err}</div>}
      {msg && <div className="notice">{msg}</div>}

      <h2>内置提示词</h2>
      <div className="card">
        {ALL_PROMPTS.map((p) => (
          <div className="row" key={p.id}>
            <span>
              {p.title}
              <span className="notice" style={{ marginLeft: 8 }}>{p.modelHint}</span>
            </span>
            <span>
              <button
                className="btn"
                style={{ margin: 0 }}
                onClick={() => setOpen(open?.id === p.id ? null : p)}
              >
                {open?.id === p.id ? '收起' : '查看'}
              </button>
              <button
                className="btn"
                style={{ margin: 0, marginLeft: 6 }}
                onClick={() => navigator.clipboard.writeText(p.body)}
              >
                复制
              </button>
            </span>
          </div>
        ))}
        {open && <textarea readOnly rows={16} value={open.body} style={{ marginTop: 10 }} />}
        <p className="notice">
          提示词全文在 <code>src/prompts/index.ts</code>，可以直接改。
          三份都刻意写成「先诊断、再报告、要确认才动手」——
          让模型直接对网络配置和软件安装动手，出错代价比多问一句大得多。
        </p>
      </div>

      <h2>还原</h2>
      <div className="card">
        <div className="row">
          <span>
            还原系统时区
            <span className="notice" style={{ marginLeft: 8 }}>
              仅当本次会话切换过时区才有效
            </span>
          </span>
          <button className="btn" style={{ margin: 0 }} onClick={restoreTz}>
            还原
          </button>
        </div>
        <div className="row">
          <span>
            收回租约并重新上锁
            <span className="notice" style={{ marginLeft: 8 }}>
              会锁住全部副本，不只是租出去的那一个
            </span>
          </span>
          <button
            className="btn"
            style={{ margin: 0 }}
            onClick={() => api.gateRelease().then(() => setMsg('已重新上锁')).catch((e) => setErr(String(e)))}
          >
            执行
          </button>
        </div>
      </div>

      <h2>合规边界</h2>
      <div className="card">
        <p className="notice" style={{ marginTop: 0 }}>
          以下四条是本项目的硬约束，任何改动都不得破坏：
        </p>
        <div className="row">1. 不读取任何限流 / 429 / 额度状态，不存在「用完自动换号」的路径</div>
        <div className="row">2. 账户切换只能由人手动触发，无定时器、无 watchdog、无自动调用点</div>
        <div className="row">3. 任意时刻只有一个账户激活</div>
        <div className="row">4. 所有账户必须是使用者本人拥有的</div>
        <p className="notice">
          卸载清理功能的定位是：修复损坏安装、移交机器、清除本人数据。
          不为规避封禁或用量限制而设计 ——
          Anthropic 政策禁止为规避限制而创建或轮换多个账户。
        </p>
      </div>

      <h2>来源与致谢</h2>
      <div className="card">
        <p className="notice" style={{ marginTop: 0 }}>
          中文环境识别改编自 FuckClaude（MIT）；DNS 检测使用 bash.ws 的公开接口；
          中转站配置形态参考 cc-switch（MIT）。逐条说明见仓库根目录 ATTRIBUTION.md。
        </p>
        <button className="btn" onClick={() => openUrl('https://github.com/LinXiaoTao/FuckClaude')}>
          FuckClaude ↗
        </button>
        <button className="btn" onClick={() => openUrl('https://github.com/farion1231/cc-switch')}>
          cc-switch ↗
        </button>
        <button className="btn" onClick={() => openUrl('https://ippure.com/')}>
          ippure.com ↗
        </button>
      </div>
    </>
  );
}
