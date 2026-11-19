/**
 * 「设置 → 中转站」：本机路由**五个软件共用**的那几样（2026-11-15，D8）。
 *
 * 原来挂在每个软件的「选线与分流」弹窗里，文案还写着「三个软件共用」—— 实际是五个
 * （Claude Code、桌面端、Codex、Gemini CLI、Grok Build）。在哪个软件的弹窗里改，五个一起变，
 * 放在每个软件底下等于告诉人「这是这个软件自己的」。
 *
 * - 监听范围与路由口令（R6）：改之前先确认（A4：原来一点单选框就保存、重启路由，不等「保存」）；
 * - 出站代理（只给中转线路，官方 Codex 那条线永远直连）；
 * - 测活：花钱测活开关（默认关）、每日预算、免费探测多久一次。
 *
 * ⛔ 代理地址框**不回填**存着的那个：后端给的是去掉了用户名密码的写法，回填之后一保存，密码就被冲掉了。
 */

import { useEffect, useRef, useState } from "react";

import {
  stationApi,
  type ProbeSettings,
  type RouterNetworkView,
} from "../../lib/station";
import { Button, useToast } from "../../ui";
import { InlineConfirm } from "./InlineConfirm";

export function RouterNetworkSettings() {
  const toast = useToast();
  const [view, setView] = useState<RouterNetworkView | null>(null);
  const [proxy, setProxy] = useState("");
  const [found, setFound] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** 选了另一档、还没确认。 */
  const [askScope, setAskScope] = useState<RouterNetworkView["scope"] | null>(
    null,
  );

  useEffect(() => {
    let dropped = false;
    void stationApi
      .routerNetwork()
      .then((v) => !dropped && setView(v))
      .catch((e: unknown) => {
        if (!dropped) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      dropped = true;
    };
  }, []);

  const run = async (what: () => Promise<RouterNetworkView>, ok: string) => {
    setBusy(true);
    setError(null);
    try {
      setView(await what());
      toast.ok(ok);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    setError(null);
    try {
      // 测的是 Claude Code 的第一条线：五个软件的中转线路走的是同一个代理。
      const ms = await stationApi.testOutboundProxy(
        "claude-code",
        proxy.trim() === "" ? null : proxy.trim(),
      );
      toast.ok(`代理通了：经它连到 Claude Code 的第一条线用了 ${ms} 毫秒`);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const scan = async () => {
    setBusy(true);
    try {
      setFound(await stationApi.scanOutboundProxies());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const copyKey = async (key: string) => {
    try {
      await navigator.clipboard.writeText(key);
      toast.ok("路由口令已复制");
    } catch {
      toast.error("复制不了，请手动选中复制");
    }
  };

  if (!view)
    return error ? (
      <div className="qb-st-nudge">读不到本机路由的网络设置：{error}</div>
    ) : (
      <p className="qb-muted">读取中…</p>
    );

  const open = view.scope === "all-interfaces";
  const shown = askScope ?? view.scope;
  return (
    <div className="qb-st-form qb-st-settings-block">
      <h3 className="qb-st-sect">监听范围</h3>
      <div role="radiogroup" aria-label="本机路由监听在哪">
        <label className="qb-st-check">
          <input
            type="radio"
            name="qb-router-scope"
            checked={shown === "loopback"}
            disabled={busy}
            onChange={() =>
              setAskScope(view.scope === "loopback" ? null : "loopback")
            }
          />
          只本机（127.0.0.1）—— 默认
        </label>
        <label className="qb-st-check">
          <input
            type="radio"
            name="qb-router-scope"
            checked={shown === "all-interfaces"}
            disabled={busy}
            onChange={() =>
              setAskScope(
                view.scope === "all-interfaces" ? null : "all-interfaces",
              )
            }
          />
          所有网卡：WSL、局域网里别的机器也能连
        </label>
      </div>
      {askScope && (
        <InlineConfirm
          danger={askScope === "all-interfaces"}
          busy={busy}
          message={
            askScope === "all-interfaces"
              ? "改成所有网卡：本机路由会重启一次（在飞的请求照样跑完），别的机器连它要带路由口令；拿到口令的人能用你的线路额度。Windows 防火墙会弹窗问一次。"
              : "改回只本机：本机路由会重启一次，别的机器从此连不上，路由口令作废。"
          }
          confirmLabel={
            askScope === "all-interfaces" ? "改成所有网卡" : "改回只本机"
          }
          onCancel={() => setAskScope(null)}
          onConfirm={() => {
            const scope = askScope;
            setAskScope(null);
            void run(
              () => stationApi.saveRouterListen(scope),
              scope === "all-interfaces"
                ? "本机路由开到了所有网卡上：别的机器要带路由口令"
                : "本机路由改成只本机能连",
            );
          }}
        />
      )}
      {open && (
        <>
          <p className="qb-st-nudge">
            <span aria-hidden="true">⚠</span>
            <span>
              别的机器连 <code>这台电脑的 IP:{view.port}</code>，API Key
              填下面这个路由口令；这台电脑上的客户端不用改。拿到口令的人就能用你的线路额度
              —— 只在你信得过的网络里开。官方 Codex 那条线不给别的机器用。
            </span>
          </p>
          <label className="qb-st-fieldrow">
            <span className="qb-st-k">路由口令</span>
            <span className="qb-st-inline">
              <input readOnly value={view.key ?? ""} aria-label="路由口令" />
              <button
                className="qb-st-iconbtn qb-st-iconbtn--text"
                onClick={() => void copyKey(view.key ?? "")}
              >
                复制
              </button>
              <button
                className="qb-st-iconbtn qb-st-iconbtn--text"
                title="旧口令立刻失效：别的机器上的客户端要改成新的"
                disabled={busy}
                onClick={() =>
                  void run(
                    () => stationApi.rotateRouterKey(),
                    "换了一个新口令，旧的已经失效",
                  )
                }
              >
                换一个
              </button>
            </span>
          </label>
        </>
      )}

      <h3 className="qb-st-sect">出站代理（只给中转线路）</h3>
      <p className="qb-st-note">
        中转线路的请求经这个代理出去（HTTP / SOCKS5）。官方 Codex 那条线永远直连
        —— 走代理的话，门禁核过的出口就不是它真走的出口。 现在：
        <b>{view.outbound_proxy ?? "直连"}</b>
      </p>
      <span className="qb-st-inline">
        <input
          value={proxy}
          aria-label="出站代理地址"
          placeholder="http://127.0.0.1:7890 或 socks5://127.0.0.1:1080"
          onChange={(e) => setProxy(e.target.value)}
        />
        <button
          className="qb-st-iconbtn qb-st-iconbtn--text"
          disabled={busy}
          title="只连本机 127.0.0.1 的几个常见端口，不发任何数据"
          onClick={() => void scan()}
        >
          扫描本机
        </button>
      </span>
      {found != null && (
        <div className="qb-st-presets">
          {found.length === 0 ? (
            <span className="qb-st-note">本机常见端口上没有代理在听。</span>
          ) : (
            found.map((u) => (
              <button key={u} className="chip" onClick={() => setProxy(u)}>
                {u}
              </button>
            ))
          )}
        </div>
      )}
      <div className="qb-st-inline" style={{ justifyContent: "flex-end" }}>
        <Button
          variant="ghost"
          disabled={busy || (proxy.trim() === "" && !view.outbound_proxy)}
          onClick={() => void test()}
        >
          测试
        </Button>
        <Button
          variant="ghost"
          disabled={busy || !view.outbound_proxy}
          onClick={() =>
            void run(
              () => stationApi.saveOutboundProxy(null),
              "出站代理清掉了：中转线路直连",
            )
          }
        >
          清掉，直连
        </Button>
        <Button
          variant="primary"
          disabled={busy || proxy.trim() === ""}
          onClick={() =>
            void run(async () => {
              const v = await stationApi.saveOutboundProxy(proxy.trim());
              setProxy("");
              return v;
            }, "出站代理已保存：下一发中转请求就经它走")
          }
        >
          保存代理
        </Button>
      </div>
      {error && (
        <div className="qb-st-nudge">
          <span aria-hidden="true">⚠</span>
          <span>{error}</span>
        </div>
      )}
    </div>
  );
}

/**
 * 测活设置（五个软件共用一份）。⛔ 花钱测活默认关，开了也按每日预算封顶 —— 这是唯一一个会**自动**花钱的入口。
 *
 * 读的是 `station_dispatch_view` 带回来的那一份（测活设置没有单独的读命令；全局只有一份，问哪个软件都一样）。
 * 改了点「保存测活设置」才落盘，不边改边存。
 */
export function ProbeSettingsPanel() {
  const toast = useToast();
  const [saved, setSaved] = useState<ProbeSettings | null>(null);
  const [draft, setDraft] = useState<ProbeSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mounted = useRef(true);
  // ⛔ 挂上时要设回 true：开发模式（StrictMode）会先挂、卸、再挂一次，只在卸下时置 false 的话，
  // 第二次挂上之后它一直是 false，读回来的设置永远落不到界面上（停在「读取中…」）。
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    void stationApi
      .dispatchView("claude-code")
      .then((v) => {
        if (!mounted.current) return;
        setSaved(v.settings);
        setDraft(v.settings);
      })
      .catch(
        (e: unknown) =>
          mounted.current &&
          setError(e instanceof Error ? e.message : String(e)),
      );
  }, []);

  if (!draft || !saved)
    return error ? (
      <div className="qb-st-nudge">读不到测活设置：{error}</div>
    ) : (
      <p className="qb-muted">读取中…</p>
    );

  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await stationApi.saveProbeSettings(draft);
      if (!mounted.current) return;
      setSaved(next);
      setDraft(next);
      toast.ok("测活设置已保存");
    } catch (e) {
      if (mounted.current)
        setError(
          `测活设置未保存：${e instanceof Error ? e.message : String(e)}`,
        );
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  return (
    <div className="qb-st-form qb-st-settings-block">
      <p className="qb-st-note">
        只测当前线路和备用线路。免费的那种（自动选线开着时一直跑）发一个空请求，只证明「线通、Key
        认」，测不出快慢；最近走过真实流量的线不测 —— 真实流量本身就是证据。
      </p>
      <div className="qb-st-floors">
        <div className="qb-st-floor">
          <input
            type="checkbox"
            checked={draft.paid}
            aria-label="开启花钱测活"
            onChange={(e) => setDraft({ ...draft, paid: e.target.checked })}
          />
          <label>
            花钱测活（默认关）
            <span style={{ fontSize: 11, color: "var(--text-3)" }}>
              {" "}
              真打一发最小的请求（输入 hi、输出 1 个
              token），量出谁快；这是唯一一个会自动花钱的地方
            </span>
          </label>
        </div>
        <div className={`qb-st-floor${draft.paid ? "" : " off"}`}>
          <span />
          <label>
            每天最多花 ¥{" "}
            <input
              className="qb-st-num"
              type="number"
              step={0.5}
              min={0}
              aria-label="每天最多花多少元"
              value={draft.daily_budget_cny}
              onChange={(e) =>
                setDraft({
                  ...draft,
                  daily_budget_cny: Number(e.target.value),
                })
              }
            />
          </label>
        </div>
        <div className="qb-st-floor">
          <span />
          <label>
            免费探测每{" "}
            <input
              className="qb-st-num"
              type="number"
              step={1}
              min={1}
              aria-label="免费探测间隔（分钟）"
              value={draft.free_interval_min}
              onChange={(e) =>
                setDraft({
                  ...draft,
                  free_interval_min: Math.max(1, Number(e.target.value)),
                })
              }
            />{" "}
            分钟一次（判死了的按 5 → 60 分钟退避）
          </label>
        </div>
      </div>
      {error && <div className="qb-st-nudge">{error}</div>}
      <div className="qb-st-inline" style={{ justifyContent: "flex-end" }}>
        <Button
          variant="ghost"
          disabled={busy || !dirty}
          onClick={() => setDraft(saved)}
        >
          撤销改动
        </Button>
        <Button
          variant="primary"
          disabled={busy || !dirty}
          onClick={() => void save()}
        >
          保存测活设置
        </Button>
      </div>
    </div>
  );
}
