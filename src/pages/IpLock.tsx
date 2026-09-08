import { useEffect, useState } from 'react';
import { api, type GateStatus, type Risk } from '../lib/api';
import type { StepApi } from '../App';
import StepFooter from '../components/StepFooter';

export default function IpLock(step: StepApi) {
  const [st, setSt] = useState<GateStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState('');

  async function refresh() {
    setErr('');
    try {
      setSt(await api.gateStatus());
    } catch (e) {
      setErr(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    const t = setInterval(refresh, 15000);
    return () => clearInterval(t);
  }, []);

  async function act(fn: () => Promise<unknown>) {
    setBusy(true);
    setErr('');
    try {
      await fn();
      await refresh();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function removeEntry(ip: string) {
    if (!st) return;
    await act(() => api.allowlistWrite(st.allowlist.filter((x) => x !== ip)));
  }

  const risk: Risk = !st
    ? 'unknown'
    : st.allowlist.length === 0
      ? 'high'
      : !st.ip_allowed
        ? 'high'
        : st.stale_copies.length > 0
          ? 'medium'
          : 'low';

  return (
    <>
      <h1>第 4 步 · IP 锁</h1>
      <p className="sub">
        门禁的本体是 NTFS <code>Deny ExecuteFile</code> ACE —— 锁上之后连双击都会被系统拒绝。
      </p>

      {err && <div className="err">{err}</div>}

      {st && st.allowlist.length === 0 && (
        <div className="card warn">
          <div className="v" style={{ color: 'var(--warn)' }}>白名单是空的，门禁尚未启用</div>
          <p className="notice" style={{ color: 'var(--warn)' }}>
            空名单意味着任何 IP 都过不了门禁，所以面板**启动时不会上锁** ——
            否则你会被自己的工具关在门外。先把当前 IP 加进来再启用。
          </p>
        </div>
      )}

      <div className="grid4">
        <div className="metric">
          <span className="k">当前出口</span>
          <span className="v mono">{st?.current_ip ?? '查不到'}</span>
        </div>
        <div className="metric">
          <span className="k">是否在白名单</span>
          <span className="v">
            {st?.ip_allowed ? (
              <span className="pill ok">是</span>
            ) : (
              <span className="pill bad">否</span>
            )}
          </span>
        </div>
        <div className="metric">
          <span className="k">执行锁</span>
          <span className="v">
            {st?.all_locked ? '全部已锁' : st?.lease.holder ? '已租出' : '部分未锁'}
          </span>
        </div>
        <div className="metric">
          <span className="k">看门狗</span>
          <span className="v">{st?.watchdog_running ? '运行中' : '未运行'}</span>
        </div>
      </div>

      <h2>白名单</h2>
      <div className="card">
        {st?.allowlist.length ? (
          st.allowlist.map((ip) => (
            <div className="row" key={ip}>
              <span className="v mono">
                {ip}
                {ip === st.current_ip && <span className="pill ok">当前</span>}
              </span>
              <button className="btn" onClick={() => removeEntry(ip)} disabled={busy}>
                删除
              </button>
            </div>
          ))
        ) : (
          <div className="empty">白名单为空 —— 任何 IP 都不会被放行。</div>
        )}
        <button
          className="btn primary"
          onClick={() => act(api.allowlistAddCurrent)}
          disabled={busy || !st?.current_ip}
          style={{ marginTop: 8 }}
        >
          把当前 IP 加进白名单
        </button>
      </div>

      <h2>受管可执行文件</h2>
      <div className="card">
        {st?.targets.length ? (
          st.targets.map((t) => (
            <div className="row" key={t.path}>
              <span className="v mono" style={{ fontSize: 11 }}>
                {t.path}
              </span>
              <span>
                <span className="pill neutral">{t.kind}</span>
                {t.locked ? (
                  <span className="pill ok">已锁</span>
                ) : (
                  <span className="pill warn">未锁</span>
                )}
              </span>
            </div>
          ))
        ) : (
          <div className="empty">没有找到可锁的 Claude 可执行文件。</div>
        )}
        <p className="notice">
          <code>app-&lt;版本&gt;\claude.exe</code> 不在列表里 ——
          给它加 Deny ACE 会让桌面端开新窗口就崩，只能靠看门狗 taskkill 收。
          这是已知残留缺口：刻意进那个目录双击能绕开启动门禁，20 秒内会被看门狗收掉，
          前提是当时有看门狗在跑。要彻底堵死需要 AppLocker / WDAC。
        </p>
        <button className="btn" onClick={() => act(api.gateLockAll)} disabled={busy}>
          立即全部上锁
        </button>
        <button
          className="btn danger"
          disabled={busy}
          onClick={() => act(api.gateUnlockAll)}
          title="不验 IP，无条件摘掉所有执行锁"
        >
          应急解锁
        </button>
        <p className="notice">
          <strong>应急解锁</strong>不验 IP，是故意留的逃生口 ——
          白名单为空、查不到公网 IP、IP 填错了，这几种情况都会让正常入口永远过不了，
          那时 claude.exe 锁着而你打不开它。安全上不吃亏：能点这个按钮的人
          本来就能改白名单文件、也能自己改 ACL；门禁防的是「跑起来之后出口 IP
          悄悄变了」，不是防本机管理员。
        </p>
      </div>

      {st && st.stale_copies.length > 0 && (
        <div className="card danger">
          <div className="v" style={{ color: 'var(--danger)' }}>
            发现 {st.stale_copies.length} 个升级残留副本
          </div>
          <p className="notice" style={{ color: 'var(--danger)' }}>
            这些副本**没有 Deny ACL**，是可以绕过门禁的执行副本。
            注意：如果某一份正被运行中的会话占着，会删不掉 ——
            那是正常的，关掉对应进程后再清一次。
          </p>
          {st.stale_copies.map((p) => (
            <div className="row mono" key={p} style={{ fontSize: 11 }}>
              {p}
            </div>
          ))}
          <button className="btn danger" onClick={() => act(api.gateCleanStale)} disabled={busy}>
            清理残留副本
          </button>
        </div>
      )}

      <h2>看门狗</h2>
      <div className="card">
        <div className="row">
          <span>Claude Code / 桥接</span>
          <span className="notice">
            每 15 秒；查不到 IP 先上锁保住进程，180 秒后才关停
          </span>
        </div>
        <div className="row">
          <span>Claude 桌面端</span>
          <span className="notice">
            每 20 秒；查不到 IP <strong>立即关闭，不给宽限</strong>
          </span>
        </div>
        <p className="notice">
          桌面端不给宽限是刻意的：它冻不住，「等等看」的实际含义就是
          「让它在无法核实的网络上继续跑」。代价是 VPN 重连或查询服务限流
          会直接关掉正在用的桌面端，丢掉没保存的对话。
        </p>
        <button
          className="btn primary"
          onClick={() => act(() => api.watchdogStart('Cli'))}
          disabled={busy || st?.watchdog_running}
        >
          启动看门狗（CLI 档）
        </button>
        <button className="btn" onClick={() => act(api.watchdogStop)} disabled={busy}>
          停止看门狗
        </button>
      </div>

      <h2>ip-gate.log</h2>
      <div className="card">
        <div className="logbox">
          {st?.recent_log.length ? st.recent_log.join('\n') : '（暂无记录）'}
        </div>
        <p className="notice">只记时间、公网 IP 与上锁解锁动作，不含任何对话内容。</p>
      </div>

      <StepFooter
        {...step}
        step="iplock"
        risk={risk}
        detail={
          st
            ? st.allowlist.length === 0
              ? '白名单为空，门禁不会放行任何 IP'
              : st.ip_allowed
                ? `当前 IP ${st.current_ip} 在白名单内`
                : `当前 IP ${st.current_ip ?? '未知'} 不在白名单内`
            : '尚未检测'
        }
        canAdvance={st !== null}
      />
    </>
  );
}
