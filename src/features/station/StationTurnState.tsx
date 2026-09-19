/**
 * 官方 Codex 的 turn-state 采集 / 注入（**实验功能，默认关闭，只对 Codex**）。
 *
 * 机制在 Rust（`qb-station::turnstate` + `qb-app::local_router` 的官方模式 +
 * `qb-app::usecase::turnstate_ops` 的槽位接管），这里只负责开关、账号规则和状态展示。
 * 0.24.7 起「开启识别」**直接作用于当前激活的 Codex 账户槽位**：临时把它的 `config.toml`
 * 指向本机路由（备份 + 增量改，`auth.json` 不碰），不另起 Codex、不复制 OAuth ——
 * 起 Codex 仍是账户页那颗按钮的事。**长度只是经验筛选口径，不是质量或额度指标** ——
 * 界面照这个口径写，不承诺「拿到 292」。详见 DISCLAIMER §5.3 与 ATTRIBUTION 的
 * ccodex-sleep-state 一节。
 */

import { useCallback, useEffect, useState } from "react";

import type { TurnStateModelStatus } from "../../lib/generated/TurnStateModelStatus";
import { stationApi } from "../../lib/station";
import { Button } from "../../ui";

function remaining(seconds: bigint): string {
  const s = Number(seconds);
  if (!Number.isFinite(s) || s <= 0) return "—";
  if (s < 60) return `${s}s`;
  return `${Math.floor(s / 60)}m ${s % 60}s`;
}

export default function StationTurnState() {
  const [enabled, setEnabled] = useState(false);
  const [team, setTeam] = useState(false);
  const [rows, setRows] = useState<TurnStateModelStatus[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  /** 官方 Codex 上游接进路由了没（内存里的路由状态）。从后端 routerStatus 读，不本地臆测。 */
  const [armed, setArmed] = useState(false);
  /** 识别接管了哪个槽位（落盘 marker 里的槽位标签；null = 没开）。这才是「识别开着没」的真相。 */
  const [takeover, setTakeover] = useState<string | null>(null);
  /** 正在开启 / 关闭。 */
  const [connecting, setConnecting] = useState(false);
  /** 刚开启成功：提示去账户页启动（或重启）Codex。 */
  const [connected, setConnected] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setRows(await stationApi.turnstateStatus());
      // 「已接入」「开关开着没」「个人还是 Team」的真相都在后端路由状态里，
      // 不由本地状态推断 —— 面板重开、换页之后本地猜的值会跟后端对不上，
      // 界面上显示「关」而实际在注。
      try {
        const rs = await stationApi.routerStatus("codex");
        setArmed(rs.turnstate_official_armed);
        setTakeover(rs.turnstate_takeover);
        setEnabled(rs.turnstate_enabled);
        setTeam(rs.turnstate_team);
      } catch {
        // 路由没起时读不到，当作未接入 —— 不让整块报错。
        setArmed(false);
      }
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 5000);
    return () => clearInterval(timer);
  }, [refresh]);

  const apply = useCallback(
    async (nextEnabled: boolean, nextTeam: boolean) => {
      setBusy(true);
      try {
        await stationApi.turnstateConfigure(nextEnabled, nextTeam);
        setEnabled(nextEnabled);
        setTeam(nextTeam);
        setError(null);
        await refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setBusy(false);
      }
    },
    [refresh],
  );

  /**
   * 开启识别：挂官方上游 → 起路由 → 接管当前激活槽位的 `config.toml`。
   *
   * ⛔ 走后端 `station_turnstate_enable` 一步到位，前端不拆步骤。成功后必须提示
   * 「到账户页启动 Codex；已开着的先关再开」—— 开着的旧 Codex 是启动时读的配置，
   * 不会自己改走路由。同一个槽位重复点是幂等的（Codex 若把配置改回去了，再改过来）。
   */
  const connect = useCallback(async () => {
    if (connecting) return;
    setConnecting(true);
    setError(null);
    try {
      const rs = await stationApi.turnstateEnable();
      setArmed(rs.turnstate_official_armed);
      setTakeover(rs.turnstate_takeover);
      setConnected(true);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setConnecting(false);
    }
  }, [connecting, refresh]);

  /**
   * 关闭识别：摘掉官方上游，并按 marker 把那个槽位的 `config.toml` 反向恢复。
   * 0.24.6 之前只有「接入」没有「断开」，使用者被锁死在识别与出站插件之间；
   * 现在恢复失败会保留 marker 并报错，再点一次即可 —— 不留「路由摘了、配置还指着路由」的半成品。
   * 不动正在跑的那个 Codex：它接下来的请求会如实收到 503，下面提示重启它。
   */
  const disconnect = useCallback(async () => {
    if (connecting) return;
    setConnecting(true);
    setError(null);
    try {
      const rs = await stationApi.turnstateDisable();
      setArmed(rs.turnstate_official_armed);
      setTakeover(rs.turnstate_takeover);
      setConnected(false);
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setConnecting(false);
    }
  }, [connecting, refresh]);

  /** 识别开着 = marker 在（或路由上还挂着官方上游 —— 两者理应一致，不一致时也算开着，好让人能关）。 */
  const on = takeover !== null || armed;

  return (
    <div style={{ display: "grid", gap: 12 }}>
      <p className="qb-st-note">
        实验功能，默认关闭，<strong>只作用于官方 Codex</strong>（Claude
        没有这个机制）。 它从你自己请求的响应里<strong>被动</strong>采集
        turn-state（不额外发请求、不消耗额度），
        并在客户端自己没带时补注一张；带了就保留你自己的。
        <strong>
          长度只是经验筛选口径，不是模型质量或额度指标，也不保证能「拿到 292」。
        </strong>
        开启后，<strong>当前激活的 Codex 账户槽位</strong>的 config.toml
        会被临时指向本机路由（先备份、只改 model_provider
        两处，登录文件一个字不碰，关闭时反向恢复）。你的 ChatGPT
        登录（OAuth）随请求流经本机回环路由（仍只绑
        127.0.0.1、不改系统代理、不做链式转发）。详见 DISCLAIMER §5.3。
      </p>

      {/* 开启 / 关闭识别。⛔ 不另起 Codex、不复制 OAuth：起 Codex 仍是账户页那颗按钮的事。
          OAuth 随请求流经本机路由，是「路由不承载官方身份」的唯一受控例外。 */}
      <div
        style={{
          display: "flex",
          gap: 10,
          alignItems: "center",
          flexWrap: "wrap",
          padding: 12,
          borderRadius: "var(--radius-md)",
          border: "1px solid var(--border)",
          background: "var(--surface-2)",
        }}
      >
        <div style={{ flex: 1, minWidth: 200 }}>
          <div style={{ fontWeight: 600 }}>
            {takeover !== null
              ? `识别已开启 · 作用于槽位「${takeover}」`
              : on
                ? "识别已开启（路由上挂着官方上游）"
                : "开启识别（作用于当前激活的 Codex 槽位）"}
          </div>
          <div style={{ color: "var(--text-2)", fontSize: "var(--text-sm)" }}>
            {connected
              ? "已开启。到账户页「Codex 桌面端」启动 Codex；已经开着的先一键关闭再启动，新会话才会走本机路由。"
              : on
                ? "面板关掉即停：本机路由随面板消失，下次启动面板会自动关闭识别并恢复槽位配置。切换槽位前先关闭识别。"
                : "需要当前槽位已用 ChatGPT 登录。开启后到账户页启动 Codex（不会替你起进程）。"}
          </div>
        </div>
        <Button
          variant="primary"
          disabled={connecting || busy}
          onClick={() => void connect()}
        >
          {connecting
            ? "处理中…"
            : on
              ? "重新应用到当前槽位"
              : "开启识别（作用于当前槽位）"}
        </Button>
        {on && (
          <Button
            variant="danger"
            disabled={connecting || busy}
            onClick={() => void disconnect()}
          >
            关闭识别（恢复槽位配置）
          </Button>
        )}
      </div>
      {on && (
        <p className="qb-st-note">
          关闭会把官方线从本机路由摘掉并恢复槽位的 config.toml；那个走路由的
          Codex 还开着的话，它接下来会收到 503——请重启它。要用扩展中心的「Codex
          出站与换出口」插件，必须先关闭这里（两者都要改同一个槽位的配置，一次只开一个）。
        </p>
      )}

      <div
        style={{
          display: "flex",
          gap: 8,
          alignItems: "center",
          flexWrap: "wrap",
        }}
      >
        <Button
          variant={enabled ? "primary" : "ghost"}
          disabled={busy}
          onClick={() => void apply(!enabled, team)}
        >
          {enabled ? "注入：开（点此关闭）" : "注入：关（点此开启）"}
        </Button>
        <span style={{ display: "inline-flex", gap: 6 }}>
          <Button
            variant={!team ? "primary" : "ghost"}
            disabled={busy}
            onClick={() => void apply(enabled, false)}
          >
            个人 · 10 块 / 292
          </Button>
          <Button
            variant={team ? "primary" : "ghost"}
            disabled={busy}
            onClick={() => void apply(enabled, true)}
          >
            Team · 12 块 / 332
          </Button>
        </span>
        <span style={{ flex: 1 }} />
        <Button variant="ghost" onClick={() => void refresh()}>
          刷新
        </Button>
      </div>

      {error && <div className="qb-st-nudge">读取状态失败：{error}</div>}

      {rows.length === 0 ? (
        <p className="qb-st-note">
          还没有任何 model 的 turn-state。把 Codex
          指向本机路由、发一条消息之后， 这里会显示采到的外形（块数 / 长度 /
          剩余时间）。
        </p>
      ) : (
        <div style={{ overflowX: "auto" }}>
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              fontSize: "var(--text-sm)",
            }}
          >
            <thead>
              <tr>
                {["模型", "状态", "块 / 长度", "剩余", "strikes", "观测"].map(
                  (h, i) => (
                    <th
                      key={h}
                      style={{
                        textAlign: i === 0 ? "left" : "right",
                        padding: "4px 8px",
                        color: "var(--text-2)",
                        fontWeight: 500,
                        whiteSpace: "nowrap",
                      }}
                    >
                      {h}
                    </th>
                  ),
                )}
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr
                  key={r.model}
                  style={{ borderTop: "1px solid var(--border)" }}
                >
                  <td style={{ padding: "4px 8px", whiteSpace: "nowrap" }}>
                    {r.model}
                  </td>
                  <td style={{ padding: "4px 8px", textAlign: "right" }}>
                    {r.status.usable
                      ? "可用"
                      : r.status.ready
                        ? "备用就绪"
                        : "无"}
                  </td>
                  <td
                    style={{
                      padding: "4px 8px",
                      textAlign: "right",
                      fontFamily: "var(--font-mono)",
                    }}
                  >
                    {r.status.blocks || "—"} / {r.status.length || "—"}
                  </td>
                  <td style={{ padding: "4px 8px", textAlign: "right" }}>
                    {remaining(r.status.remaining_seconds)}
                  </td>
                  <td style={{ padding: "4px 8px", textAlign: "right" }}>
                    {r.status.strikes}
                  </td>
                  <td style={{ padding: "4px 8px", textAlign: "right" }}>
                    {String(r.status.observations)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
