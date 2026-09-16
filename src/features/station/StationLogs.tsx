/**
 * 请求日志弹窗（§2.7，1180px）。
 *
 * **只记状态码、时延、token，不记正文。** 这条写在界面上而不是只写在注释里 ——
 * 使用者有权知道一个会看到全部请求的组件到底留了什么。
 *
 * 取日志这一步顺带把内存里攒的那批落库（后端 `station_logs` 就是这么做的），
 * 所以这一页刷新一次 = 一次落库。
 *
 * # ⛔ 两个数据源必须分行标注
 *
 * 这一页上的数字来自**两处**，口径不同，算出来的成功率不会完全一致：
 *
 * | 来源 | 谁写的 | 覆盖什么 |
 * |---|---|---|
 * | 路由日志 | 本机路由自己 | 只有经过本机路由的请求 |
 * | 站点账单 | 中转站 | 这条线上的全部请求，包括没走本机路由的 |
 *
 * 不标来源就是自相矛盾：同一张卡上两个成功率不一样，而没有任何地方说得清
 * 哪个对。Token 四类只有账单那边有（路由不解析响应体），所以那一段一定是账单口径。
 *
 * # ⛔ 没有「来源 / 客户端」筛选
 *
 * 弹窗已经开在某一条线路底下了，这里全是它的记录。再给一个客户端筛选，
 * 选出来永远是同一批 —— 一个永远不改变结果的控件比没有这个控件更糟。
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import type { Credential } from "../../lib/generated/Credential";
import {
  rateCell,
  stationApi,
  type RequestLog,
  type Route,
  type RouteHealthView,
} from "../../lib/station";
import { Button } from "../../ui";
import {
  Chart,
  dash,
  mtok,
  pct,
  RANGES,
  rangeOf,
  type RangeId,
} from "./shared";

/**
 * 库里最多留这么多条。
 *
 * ⛔ 跟 Rust 侧 `station_ops::REQUEST_LOG_KEEP` 是**同一个数**。对不上的话，
 * 界面会在还没到边界时就说「更早的被裁掉了」，或者更糟 —— 到了边界却不说，
 * 让人以为「近 7 天」就这点请求。
 */
const LOG_KEEP = 2000;

export default function StationLogs({
  route,
  health,
  siteUrl,
  credentials,
}: {
  route: Route;
  /** 站点账单口径的 24h 窗口。`undefined` = 这一轮没拉到。 */
  health?: RouteHealthView;
  siteUrl: string;
  credentials: Credential[];
}) {
  const [rows, setRows] = useState<RequestLog[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [range, setRange] = useState<RangeId>("24h");
  const [failedOnly, setFailedOnly] = useState(false);
  const [model, setModel] = useState("");

  const load = useCallback(async () => {
    setBusy(true);
    try {
      setRows(await stationApi.logs(LOG_KEEP));
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // 这条线路的全部记录（不看时间档）—— 边界提示要拿它算。
  const mine = useMemo(
    () => rows.filter((r) => r.route_id === route.id),
    [rows, route.id],
  );

  const models = useMemo(
    () => [...new Set(mine.map((r) => r.model).filter(Boolean))].sort(),
    [mine],
  );

  const since = Date.now() - rangeOf(range).hours * 3600_000;
  const inRange = useMemo(
    () => mine.filter((r) => Number(r.at_ms) >= since),
    [mine, since],
  );
  const shown = useMemo(
    () =>
      inRange.filter(
        (r) =>
          (!failedOnly || !ok(r.status)) && (model === "" || r.model === model),
      ),
    [inRange, failedOnly, model],
  );

  const stats = useMemo(() => summarise(inRange), [inRange]);

  // ⛔ 库里只留 2000 条。选了「近 7 天」而最早那条只到 3 天前时，
  // 必须写明是**被裁掉了**，不能让人以为 7 天就这点请求。
  const oldest = mine.length
    ? Math.min(...mine.map((r) => Number(r.at_ms)))
    : null;
  const truncated =
    rows.length >= LOG_KEEP && oldest != null && oldest > since ? oldest : null;

  const cred = credentials.find((c) => c.id === route.credential_id);

  return (
    <>
      {/* ---------------------------------------------- 一、线路配置带 */}
      <div className="qb-st-logband">
        <BandItem k="倍率">
          <BandRate route={route} />
        </BandItem>
        <BandItem k="绑定 Key">
          {/* ⛔ 三态：没绑 / 绑了但那把令牌已经不在了 / 绑着。
              中间那一档不能显示成「没绑令牌」—— 使用者明明绑过，
              显示成没绑会让他再绑一次，而真正的问题是那把 key 被删了。 */}
          {cred ? (
            <>
              {cred.label}
              {!cred.available && (
                <span className="qb-st-pill qb-st-pill--danger">读不出来</span>
              )}
            </>
          ) : route.credential_id ? (
            <span className="qb-st-pill qb-st-pill--danger">令牌已被删除</span>
          ) : (
            <span className="qb-st-dim">没绑令牌</span>
          )}
        </BandItem>
        <BandItem k="协议">
          <Protocols route={route} />
        </BandItem>
        <BandItem k="站点地址">
          <span className="qb-st-mono">{siteUrl || dash}</span>
        </BandItem>
      </div>

      {error && <div className="qb-st-nudge">读不到请求日志：{error}</div>}

      {/* ---------------------------------------------- 二、统计 + 曲线 + token */}
      <div className="qb-st-logstats">
        <div className="qb-card">
          <div className="qb-st-srcline">
            <b>路由日志口径</b>
            <span>只统计经过本机路由的请求 · {rangeOf(range).name}</span>
          </div>
          <div className="qb-st-logkpis">
            <Kpi k="请求" v={num(stats.total)} />
            <Kpi k="成功" v={num(stats.ok)} />
            <Kpi k="失败" v={num(stats.total - stats.ok)} s={stats.byCode} />
            <Kpi k="首字 P50" v={secs(stats.p50)} />
            <Kpi k="首字 P95" v={secs(stats.p95)} />
            <Kpi k="最慢首字" v={secs(stats.worst)} />
            <Kpi k="耗时中位" v={secs(stats.totalMedian)} />
          </div>
          <Chart rows={inRange} range={range} gran="hour" />
          <div className="qb-st-legend">
            <span>
              <i style={{ background: "var(--accent)" }} />
              请求量
            </span>
            <span>
              <i style={{ background: "var(--warn)" }} />
              首字 P95
            </span>
            <span>各按自己的量程</span>
          </div>
        </div>

        <div className="qb-card">
          <div className="qb-st-srcline">
            <b>站点账单口径</b>
            <span>
              中转站自己记的 · 近 24 小时 ·
              包含没走本机路由的那些，所以跟左边对不上是正常的
            </span>
          </div>
          <TokenBar health={health} />
          <div className="qb-st-logkpis">
            <Kpi k="成功率" v={pct(health?.success_rate)} />
            <Kpi k="缓存命中" v={pct(health?.cache_hit_rate)} />
            <Kpi
              k="实扣"
              v={health?.cost_24h == null ? "—" : health.cost_24h.toFixed(4)}
            />
          </div>
          {health?.error && (
            <p className="qb-st-note">
              {health.error} —— 上面几项是<b>没有数据</b>，不是零。
            </p>
          )}
        </div>
      </div>

      {/* ---------------------------------------------- 三、日志表 */}
      <div className="qb-card">
        <div className="qb-st-logfilter">
          <div className="qb-st-seg" role="group" aria-label="时间范围">
            {RANGES.map((r) => (
              <button
                key={r.id}
                aria-pressed={range === r.id}
                onClick={() => setRange(r.id)}
              >
                {r.name}
              </button>
            ))}
          </div>
          <label className="qb-st-check">
            <input
              type="checkbox"
              checked={failedOnly}
              onChange={(e) => setFailedOnly(e.target.checked)}
            />
            只看失败
          </label>
          <label className="qb-st-field">
            模型
            <select value={model} onChange={(e) => setModel(e.target.value)}>
              <option value="">全部</option>
              {models.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          </label>
          <span style={{ flex: 1 }} />
          <Button variant="ghost" disabled={busy} onClick={load}>
            刷新
          </Button>
        </div>

        {truncated && (
          <p className="qb-st-note">
            ⚠ 库里只留最近 {LOG_KEEP.toLocaleString()} 条， 这条线路最早的一条是{" "}
            <b>{full(truncated)}</b>。 再往前的已经被裁掉了 ——{" "}
            <b>不是这段时间没有请求</b>。
          </p>
        )}

        <div className="qb-st-tw">
          <table>
            <thead>
              <tr>
                <th>时间</th>
                <th>模型</th>
                <th>状态</th>
                <th className="num">首字</th>
                <th className="num">耗时</th>
                <th className="num">Token</th>
                <th className="num">Retry-After</th>
              </tr>
            </thead>
            <tbody>
              {shown.length === 0 && (
                <tr>
                  <td colSpan={7} style={{ color: "var(--text-3)" }}>
                    {mine.length === 0
                      ? "这条线路还没有请求。本机路由启动、客户端把地址指过来之后，每一发都会记在这里。"
                      : "这一档里没有符合筛选条件的记录。"}
                  </td>
                </tr>
              )}
              {shown.map((r) => (
                <tr key={r.id}>
                  <td>{full(Number(r.at_ms))}</td>
                  <td>{r.model || dash}</td>
                  <td>
                    <StatusPill log={r} />
                  </td>
                  <td className="num">{secs(numOrNull(r.first_token_ms))}</td>
                  <td className="num">{secs(numOrNull(r.total_ms))}</td>
                  {/* ⛔ 路由不解析响应体，所以这里没有 token 数。
                      写 0 会被读成「这一发没花 token」—— 那是个断言。 */}
                  <td className="num">{dash}</td>
                  <td className="num">
                    {r.retry_after_ms == null
                      ? dash
                      : `${Math.round(Number(r.retry_after_ms) / 1000)} s`}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="qb-st-note" style={{ marginTop: 9 }}>
          只记状态码、时延、<b>不记正文</b>。Token
          那一列空着是因为路由不拆响应体 —— 分类 token
          只有站点账单里有，在上面那张卡里。
        </p>
      </div>
    </>
  );
}

// ------------------------------------------------------------------ 配置带

function BandItem({ k, children }: { k: string; children: React.ReactNode }) {
  return (
    <div className="qb-st-banditem">
      <span className="k">{k}</span>
      <span className="v">{children}</span>
    </div>
  );
}

/** 倍率三态：未核实 / 已核实 / 实测对不上（标称划掉 + 实测标红）。 */
function BandRate({ route }: { route: Route }) {
  const cell = rateCell(route);
  if (cell.kind === "verified")
    return (
      <>
        ×{cell.rate.toFixed(2)}
        <span className="qb-st-pill qb-st-pill--ok">已核实</span>
      </>
    );
  if (cell.kind === "unverified")
    return (
      <>
        {cell.nominal == null ? dash : `×${cell.nominal.toFixed(2)}`}
        <span className="qb-st-pill qb-st-pill--unknown">标称 · 未核实</span>
      </>
    );
  return (
    <>
      <s className="qb-st-dim">×{cell.nominal.toFixed(2)}</s>{" "}
      <b className="qb-st-bad">×{cell.real.toFixed(2)}</b>
      <span className="qb-st-pill qb-st-pill--danger">
        {cell.overcharging ? "实测超收" : "实测"}
      </span>
    </>
  );
}

/**
 * 协议三态。
 *
 * ⛔ `null`（还没探过）跟 `false`（探过、不支持）是两回事，界面上必须分得开 ——
 * 混成一个的话，一条没探过的线会显示成「不支持」，人就去改配置了。
 */
function Protocols({ route }: { route: Route }) {
  const p = route.protocols;
  const all: [string, boolean | null][] = [
    ["Anthropic", p.anthropic],
    ["OpenAI Chat", p.openai_chat],
    ["OpenAI Responses", p.openai_responses],
  ];
  return (
    <>
      {all.map(([name, v]) => (
        <span
          key={name}
          className={`qb-st-pill ${
            v === true
              ? "qb-st-pill--ok"
              : v === false
                ? "qb-st-pill--danger"
                : "qb-st-pill--unknown"
          }`}
          title={
            v === true
              ? "探通了"
              : v === false
                ? "探过，上游明确拒绝"
                : "还没探过 —— 不是「不支持」"
          }
        >
          {name}
          {v === null ? " ?" : v ? " ✓" : " ✕"}
        </span>
      ))}
    </>
  );
}

// ------------------------------------------------------------------ Token 堆叠条

/**
 * 四类 token 堆叠条。
 *
 * ⛔ **四类分开画。** 合成一个总数会把「输出翻五倍」整个藏掉 ——
 * 而输出往往才是花钱的大头。
 *
 * ⛔ 一类都没取证到就如实说，不要把缺的按 0 画 —— 缺一类会让占比整个偏掉，
 * 而偏的方向恰好是「这家看起来更便宜」。
 */
function TokenBar({ health }: { health?: RouteHealthView }) {
  const parts: [string, number | null, string][] = [
    ["输入", numOrNull(health?.input_tokens), "var(--accent)"],
    ["缓存读", numOrNull(health?.cache_read_tokens), "var(--ok)"],
    ["缓存写", numOrNull(health?.cache_write_tokens), "var(--warn)"],
    ["输出", numOrNull(health?.output_tokens), "var(--danger)"],
  ];
  const known = parts.filter(
    (p): p is [string, number, string] => p[1] != null,
  );
  const total = known.reduce((a, p) => a + p[1], 0);
  if (!known.length || total <= 0)
    return (
      <div className="qb-st-chart-empty">
        站点账单没有分类明细 —— 四类都不知道，<b>不是零</b>
      </div>
    );
  return (
    <div className="qb-st-tokbar">
      <div className="bar">
        {known.map(([name, v, color]) => (
          <i
            key={name}
            style={{ width: `${(v / total) * 100}%`, background: color }}
            title={`${name} ${mtok(v)}`}
          />
        ))}
      </div>
      <div className="qb-st-legend">
        {parts.map(([name, v, color]) => (
          <span key={name}>
            <i style={{ background: color }} />
            {name} {v == null ? "—" : mtok(v)}
          </span>
        ))}
        <span style={{ flex: 1 }} />
        <span>共 {mtok(total)}</span>
      </div>
    </div>
  );
}

// ------------------------------------------------------------------ 小工具

function Kpi({
  k,
  v,
  s,
}: {
  k: string;
  /** `pct()` 空值时回的是 `—` 那个元素，不是字符串。 */
  v: React.ReactNode;
  s?: string;
}) {
  return (
    <div className="qb-st-logkpi">
      <span className="k">{k}</span>
      <b>{v}</b>
      {s && <span className="s">{s}</span>}
    </div>
  );
}

function StatusPill({ log }: { log: RequestLog }) {
  const s = log.status;
  if (s == null) return <span className="pill pill--neutral">没有回应</span>;
  if (s === 429)
    return (
      <span className="pill pill--warn">
        429 · 降权
        {log.retry_after_ms != null &&
          ` ${Math.round(Number(log.retry_after_ms) / 1000)} s`}
      </span>
    );
  if (s === 401 || s === 402 || s === 403)
    return <span className="pill pill--danger">{s} · 熔断</span>;
  if (s >= 500) return <span className="pill pill--danger">{s}</span>;
  return <span className="pill pill--ok">{s}</span>;
}

function ok(status: number | null): boolean {
  return status != null && status >= 200 && status < 400;
}

function numOrNull(v: bigint | number | null | undefined): number | null {
  return v == null ? null : Number(v);
}

function num(n: number): string {
  return n ? n.toLocaleString() : "—";
}

/** 取不到就是「—」，**不显示 0** —— 0 秒首字是个荒谬的断言。 */
function secs(ms: number | null): string {
  if (ms == null) return "—";
  return `${(ms / 1000).toFixed(2)} s`;
}

/** `09-13 22:10:07`。日志表要看得出秒 —— 同一分钟里好几发是常态。 */
function full(ms: number): string {
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return "—";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(
    d.getMinutes(),
  )}:${p(d.getSeconds())}`;
}

/** 分位数。空数组回 `null` —— **不回 0**。 */
function quantile(sorted: number[], q: number): number | null {
  if (!sorted.length) return null;
  return sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))];
}

function summarise(rows: RequestLog[]) {
  const first = rows
    .map((r) => numOrNull(r.first_token_ms))
    .filter((v): v is number => v != null)
    .sort((a, b) => a - b);
  const totals = rows
    .map((r) => numOrNull(r.total_ms))
    .filter((v): v is number => v != null)
    .sort((a, b) => a - b);
  // 失败按状态码分开数 —— 401 和 500 该做的事完全不同，合成一个「失败 7」
  // 等于让人自己再去翻表。
  const codes = new Map<string, number>();
  for (const r of rows) {
    if (ok(r.status)) continue;
    const k = r.status == null ? "没有回应" : String(r.status);
    codes.set(k, (codes.get(k) ?? 0) + 1);
  }
  return {
    total: rows.length,
    ok: rows.filter((r) => ok(r.status)).length,
    p50: quantile(first, 0.5),
    p95: quantile(first, 0.95),
    worst: first.length ? first[first.length - 1] : null,
    totalMedian: quantile(totals, 0.5),
    byCode:
      [...codes.entries()].map(([k, v]) => `${k}×${v}`).join(" · ") ||
      undefined,
  };
}
