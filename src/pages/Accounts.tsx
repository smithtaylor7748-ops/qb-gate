import { useEffect, useState } from 'react';
import { api, type Risk, type Slot } from '../lib/api';
import type { StepApi } from '../App';
import StepFooter from '../components/StepFooter';

export default function Accounts(step: StepApi) {
  const [slots, setSlots] = useState<Slot[]>([]);
  const [caveat, setCaveat] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');
  const [msg, setMsg] = useState('');

  async function refresh() {
    setErr('');
    try {
      const r = await api.accountsList();
      setSlots(r.slots);
      setCaveat(r.caveat);
    } catch (e) {
      setErr(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function act(fn: () => Promise<unknown>, ok: string) {
    setBusy(true);
    setErr('');
    setMsg('');
    try {
      await fn();
      setMsg(ok);
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  const active = slots.find((s) => s.active);
  const risk: Risk = !slots.length
    ? 'unknown'
    : !active?.logged_in
      ? 'high'
      : (active.cli_days_left ?? 99) < 5
        ? 'medium'
        : 'low';

  return (
    <>
      <h1>第 5 步 · 账户与启动</h1>
      <p className="sub">登录、切换槽位，验证出口 IP 之后再启动。</p>

      {err && <div className="err">{err}</div>}
      {msg && <div className="notice">{msg}</div>}

      <h2>账户槽位</h2>
      <div className="card">
        {slots.length ? (
          slots.map((s) => (
            <div className="row" key={s.label}>
              <span>
                {s.active ? '●' : '○'} {s.label}
                {s.active && <span className="pill ok">激活</span>}
                {!s.logged_in && <span className="pill neutral">未登录</span>}
                {s.cli_days_left !== null && s.cli_days_left !== undefined && (
                  <span
                    className={`pill ${s.cli_days_left < 0 ? 'bad' : s.cli_days_left < 5 ? 'warn' : 'ok'}`}
                  >
                    {s.cli_days_left < 0 ? '凭证已过期' : `剩 ${s.cli_days_left} 天`}
                  </span>
                )}
              </span>
              <button
                className="btn"
                disabled={busy || s.active}
                onClick={() => act(() => api.accountsSwitch(s.label), `已切换到 ${s.label}`)}
              >
                切换
              </button>
            </div>
          ))
        ) : (
          <div className="empty">没有找到账户槽位。首次使用时先登录一次即可建立。</div>
        )}
        <p className="notice">{caveat}</p>
        <p className="notice">
          凭证过期的槽位**仍然可以切换** —— 你得先切过去，才能在那个槽里重新登录。
          「已登录」和「没过期」是两件事，面板刻意不把它们合并。
        </p>
      </div>

      <h2>登录</h2>
      <div className="card">
        <p className="notice" style={{ marginTop: 0 }}>
          <code>claude.exe</code> 上常驻一条 Deny ExecuteFile，
          <strong>双击它会被系统拒绝执行，这是设计如此</strong>。
          登录必须走下面这两个受控入口 —— 它们会先验 IP 再放行。
        </p>
        <button
          className="btn primary"
          disabled={busy}
          onClick={() => act(() => api.gateOpen('claude-code-login'), '已验证 IP 并放行 Claude Code')}
        >
          验证 IP 后打开 Claude Code
        </button>
        <button
          className="btn primary"
          disabled={busy}
          onClick={() => act(() => api.gateOpen('claude-desktop-login'), '已验证 IP 并放行桌面端')}
        >
          验证 IP 后打开桌面端
        </button>
        <button className="btn" disabled={busy} onClick={() => act(api.gateRelease, '租约已收回，全部副本重新上锁')}>
          收回租约并上锁
        </button>
      </div>

      <h2>启动</h2>
      <div className="card accent">
        <p className="notice" style={{ marginTop: 0 }}>
          启动会先校验出口 IP，通过后解锁并在后台静默起看门狗。
        </p>
        <button
          className="btn primary"
          disabled={busy}
          onClick={() =>
            act(async () => {
              await api.gateOpen('claude-desktop');
              await api.watchdogStart('Desktop');
            }, '桌面端已放行，看门狗已在后台运行')
          }
        >
          启动 Claude 桌面端
        </button>
        <button
          className="btn primary"
          disabled={busy}
          onClick={() =>
            act(async () => {
              await api.gateOpen('claude-code');
              await api.watchdogStart('Cli');
            }, 'Claude Code 已放行，看门狗已在后台运行')
          }
        >
          启动 Claude Code
        </button>
      </div>

      <div className="card">
        <p className="notice" style={{ marginTop: 0 }}>
          <strong>合规边界</strong>：本面板不读取任何限流 / 429 / 额度状态，
          不存在「用完自动换号」的路径；账户切换只能由你手动点击触发，
          没有定时器也没有自动调用点；任意时刻只有一个账户激活。
          前提由你自己把关 —— <strong>所有账户必须是你本人拥有的</strong>。
        </p>
      </div>

      <StepFooter
        {...step}
        step="accounts"
        risk={risk}
        detail={
          active
            ? `当前槽位 ${active.label}${active.logged_in ? '，已登录' : '，未登录'}`
            : '没有激活的槽位'
        }
        canAdvance={slots.length > 0}
      />
    </>
  );
}
