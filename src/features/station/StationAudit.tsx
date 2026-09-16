/**
 * 站点检验：清单页 + 报告页两层。
 *
 * ⛔ **只在使用者点的时候跑,而且会计费。** 这一页没有任何自动触发。
 *
 * ⛔ **「从没检验过」是独立行态**（虚线框 + 「—」），不写成 0 分 ——
 * 0 是一个断言，没检验过是还没有断言，两者在界面上的处置完全不同。
 *
 * ⛔ **造假不等于拉黑。** 检验出对不上，结果只有一个：把真实倍率算出来，
 * 拿真实值去排序。这一页没有任何「排除 / 禁用」的按钮。
 */

import { useCallback, useEffect, useState } from "react";

import type { AuditCheck } from "../../lib/generated/AuditCheck";
import type { CategoryVerdict } from "../../lib/generated/CategoryVerdict";
import type { CheckKind } from "../../lib/generated/CheckKind";
import type { EvidenceLevel } from "../../lib/generated/EvidenceLevel";
import type { PriceCategory } from "../../lib/generated/PriceCategory";
import type { RateBasis } from "../../lib/generated/RateBasis";
import type { StoredAudit } from "../../lib/generated/StoredAudit";
import { call } from "../../lib/ipc";
import {
  groupsOf,
  modelRateLabel,
  rateCell,
  stationApi,
  stationsOf,
  type Route,
  type StationModelsView,
} from "../../lib/station";
import { Button } from "../../ui";

const KIND: Record<CheckKind, string> = {
  rate: "倍率",
  "cache-hit": "缓存命中",
  "context-window": "上下文窗口",
  "max-output": "最大输出",
  "first-token": "首字延迟",
  "success-rate": "成功率",
};

const CATEGORY: Record<PriceCategory, string> = {
  input: "输入",
  "cache-read": "缓存读",
  "cache-write": "缓存写",
  output: "输出",
};

/**
 * 站点是按哪套口径公布价格的。
 *
 * ⛔ **两套不可比,所以表头要跟着换。** New API 系公布的是相对它自己
 * 配额基准的倍率，sub2api 系公布的是绝对单价（使用者原话：「sub2api 的
 * 单价就是官方单价」）。把两者摆在同一列下面，×0.5 和 $2.5 会被读成
 * 同一种东西。
 */
const BASIS: Record<
  RateBasis,
  { head: string; pill: string; tone: string; note: string }
> = {
  ratios: {
    head: "站点倍率",
    pill: "倍率口径",
    tone: "pill--neutral",
    note: "这家站点公布的是相对它自己配额基准的倍率。这一列是它自己说的，不是量出来的 —— 「到底收了几倍」看上面那一格「倍率」的实测值。",
  },
  "absolute-prices": {
    head: "站点单价",
    pill: "绝对单价口径",
    tone: "pill--neutral",
    note: "这家站点公布的是绝对单价（美元／百万 token），折扣在分组上。右边那一列是它除以官方单价 —— 官方价真的参与了这个数。",
  },
  "per-request": {
    head: "站点单价",
    pill: "按次计费",
    tone: "pill--warn",
    note: "这家站点按次计费，四类 token 的口径整个不适用 —— 不是「没公布」，是这个模型压根不按 token 算钱。",
  },
  unknown: {
    head: "站点价",
    pill: "没公布",
    tone: "pill--neutral",
    note: "这家站点没公布价目表，或者表里没有这个模型。⛔ 空着不等于免费，也不等于倍率 1.0 —— 是不知道。",
  },
};

/**
 * 证据档次。
 *
 * ⛔ **必须跟可信度一起显示。** 29 分有两种完全相反的来源：
 * 「六项都测了、四项对不上」是这站确实有问题（该换站）；
 * 「只测到一项」是还不能下结论（该再跑一轮）。
 * 只给分数的话，两者在界面上长得一模一样。
 */
const EVIDENCE: Record<EvidenceLevel, { text: string; tone: string }> = {
  sufficient: { text: "证据充分", tone: "pill--ok" },
  partial: { text: "证据不全", tone: "pill--warn" },
  insufficient: { text: "证据不足，还不能下结论", tone: "pill--neutral" },
  conflict: { text: "多项对不上", tone: "pill--danger" },
};

function EvidencePill({ level }: { level: EvidenceLevel }) {
  const e = EVIDENCE[level];
  return <span className={`pill ${e.tone}`}>{e.text}</span>;
}

const auditApi = {
  history: (routeId: string) =>
    call<StoredAudit[]>("station_audits", { routeId }),
  /**
   * 跑一轮检验。**会计费。**
   *
   * `model` 传 null 表示让后端自己挑（站点真提供的那个默认模型）。
   * ⛔ 别在这里填一个写死的模型名兜底 —— 站点没有它的话，验的是一个不存在的
   * 东西，六项里的倍率会全记「没测到」，界面上看起来像站点不配合。
   */
  run: (routeId: string, model?: string | null) =>
    call<StoredAudit>("station_run_audit", { routeId, model: model ?? null }),
};

/**
 * 「选中转站 → 选分组 → 选模型」那三级。
 *
 * 前两级是线路池里现成的（一条线路就是站点 + 分组），第三级去站点的价目表拉。
 *
 * # ⛔ 拉不到模型表不等于不能验
 *
 * 有的站点根本没开 `/api/pricing`。那种情况下这里退化成一个输入框让使用者
 * 自己填模型名 —— 灰掉「开始检验」的话，这种站一个模型都验不了。
 */
function Cascade({
  routes,
  busy,
  onRun,
}: {
  routes: Route[];
  busy: boolean;
  onRun: (routeId: string, model: string | null) => void;
}) {
  const stations = stationsOf(routes);
  const [station, setStation] = useState(stations[0] ?? "");
  const groups = groupsOf(routes, station);
  const [routeId, setRouteId] = useState(groups[0]?.id ?? "");
  const [view, setView] = useState<StationModelsView | null>(null);
  const [model, setModel] = useState("");
  const [loading, setLoading] = useState(false);

  // 站点换了 → 分组回到第一个；分组换了 → 重新拉模型表。
  useEffect(() => {
    const first = groupsOf(routes, station)[0]?.id ?? "";
    setRouteId((cur) =>
      groupsOf(routes, station).some((r) => r.id === cur) ? cur : first,
    );
  }, [routes, station]);

  useEffect(() => {
    if (!routeId) {
      setView(null);
      setModel("");
      return;
    }
    let live = true;
    setLoading(true);
    setView(null);
    stationApi
      .models(routeId)
      .then((v) => {
        if (!live) return;
        setView(v);
        // 默认选后端建议的那个（claude-opus-5 / gpt-5.6-sol，站点有才用）。
        setModel(v.recommended ?? "");
      })
      .catch(() => {
        if (!live) return;
        setView({
          models: [],
          recommended: null,
          problem: "读不到这个站点的价目表",
        });
        setModel("");
      })
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [routeId]);

  const models = view?.models ?? [];
  const noTable = Boolean(view && models.length === 0);

  return (
    <div className="qb-card">
      <h3>挑一条线验</h3>
      <p className="qb-st-note">
        选中转站 → 选分组 → 选模型。 一个模型就够 ——
        站点要在计费上做手脚，不会只在一个模型上做。
      </p>
      <div className="qb-st-cascade">
        <label>
          <span>中转站</span>
          <select
            value={station}
            onChange={(e) => setStation(e.target.value)}
            disabled={stations.length === 0}
          >
            {stations.length === 0 && <option value="">还没有站点</option>}
            {stations.map((id) => (
              <option key={id} value={id}>
                {id}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>分组</span>
          <select
            value={routeId}
            onChange={(e) => setRouteId(e.target.value)}
            disabled={groups.length === 0}
          >
            {groups.length === 0 && <option value="">还没有分组</option>}
            {groups.map((r) => (
              <option key={r.id} value={r.id}>
                {r.group || "默认分组"}
                {r.nominal_rate != null ? ` · 标称 ×${r.nominal_rate}` : ""}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>模型</span>
          {noTable ? (
            <input
              value={model}
              placeholder="自己填一个模型名"
              onChange={(e) => setModel(e.target.value)}
            />
          ) : (
            <select
              value={model}
              onChange={(e) => setModel(e.target.value)}
              disabled={loading || models.length === 0}
            >
              {loading && <option value="">读价目表中…</option>}
              {!loading && models.length === 0 && (
                <option value="">先选一个分组</option>
              )}
              {models.map((m) => (
                <option key={m.model} value={m.model}>
                  {m.model} · {modelRateLabel(m)}
                </option>
              ))}
            </select>
          )}
        </label>
        <Button
          variant="primary"
          disabled={busy || !routeId || model.trim() === ""}
          onClick={() => onRun(routeId, model.trim() || null)}
        >
          验这一个
        </Button>
      </div>
      {view?.problem && <p className="qb-st-note">{view.problem}</p>}
      {model && view?.recommended === model && (
        <p className="qb-st-note">
          默认挑的是这个 —— 它是这个分组里最能看出问题的那档。换一个也行。
        </p>
      )}
    </div>
  );
}

export default function StationAudit({
  routes,
  focus = null,
}: {
  routes: Route[];
  /**
   * 直接落到这条线的报告，跳过清单。
   *
   * ⛔ 从账户行点「检验」进来时**必须**给它 —— 使用者在列表里已经选过一次了，
   * 弹窗里再列一遍让他选第二次，是把同一个决定问了两遍。
   * `null` 才是「从清单进来」，那时才需要选。
   */
  focus?: string | null;
}) {
  const [history, setHistory] = useState<Record<string, StoredAudit[]>>({});
  const [open, setOpen] = useState<string | null>(focus);
  const [picked, setPicked] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    const out: Record<string, StoredAudit[]> = {};
    for (const r of routes) {
      try {
        out[r.id] = await auditApi.history(r.id);
      } catch {
        out[r.id] = [];
      }
    }
    setHistory(out);
  }, [routes]);

  useEffect(() => {
    void load();
  }, [load]);

  // 同一个弹窗换一条线路时要跟着走，否则第二次点开的还是上一条。
  useEffect(() => {
    setOpen(focus);
  }, [focus]);

  const chosen = routes.filter((r) => picked[r.id]);

  const runAll = async () => {
    setBusy(true);
    setError(null);
    try {
      // 批量时不指定模型 —— 每条线路的分组不一样，后端各挑各的。
      for (const r of chosen) await auditApi.run(r.id, null);
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  if (open) {
    const rows = history[open] ?? [];
    const route = routes.find((r) => r.id === open);
    return (
      <Report
        route={route}
        rounds={rows}
        // focus 进来的没有清单可回，藏掉那个「← 回清单」。
        onBack={focus ? null : () => setOpen(null)}
        onRun={async () => {
          setBusy(true);
          try {
            await auditApi.run(open, null);
            await load();
          } finally {
            setBusy(false);
          }
        }}
        busy={busy}
      />
    );
  }

  const unaudited = routes.filter((r) => (history[r.id]?.length ?? 0) === 0);

  return (
    <>
      {unaudited.length > 0 && (
        <div className="qb-st-nudge">
          <span aria-hidden="true">⚠</span>
          <span>
            有 <b>{unaudited.length}</b> 条线路从没检验过 —— 它们的倍率用的是
            <b>站点自己标称</b>的那个数，排序和底线也按它算。
          </span>
        </div>
      )}

      {error && <div className="qb-st-nudge">检验失败：{error}</div>}

      <Cascade
        routes={routes}
        busy={busy}
        onRun={async (routeId, model) => {
          setBusy(true);
          setError(null);
          try {
            await auditApi.run(routeId, model);
            await load();
            setOpen(routeId);
          } catch (e) {
            setError(e instanceof Error ? e.message : String(e));
          } finally {
            setBusy(false);
          }
        }}
      />

      <div className="qb-card">
        <h3>检验清单</h3>
        <p className="qb-st-note">
          检验的是站点，跟哪个客户端在用它无关 —— 所以这里是全部线路。
        </p>
        <div className="qb-st-tw">
          <table>
            <thead>
              <tr>
                <th style={{ width: 26 }} />
                <th>站点 · 分组</th>
                <th>可信度</th>
                <th>上次检验</th>
                <th>哪几项对不上</th>
                <th>倍率</th>
                <th className="num" />
              </tr>
            </thead>
            <tbody>
              {routes.length === 0 && (
                <tr>
                  <td colSpan={7} style={{ color: "var(--text-3)" }}>
                    还没有线路
                  </td>
                </tr>
              )}
              {routes.map((r) => {
                const rounds = history[r.id] ?? [];
                const latest = rounds[0];
                const never = rounds.length === 0;
                const fails = latest
                  ? latest.round.checks
                      .filter((c) => c.verdict === "differs")
                      .map((c) => KIND[c.kind])
                  : [];
                const cell = rateCell(r);
                return (
                  <tr key={r.id}>
                    <td>
                      <input
                        type="checkbox"
                        checked={Boolean(picked[r.id])}
                        onChange={(e) =>
                          setPicked({ ...picked, [r.id]: e.target.checked })
                        }
                        aria-label={`选中 ${r.id}`}
                      />
                    </td>
                    <td>
                      <button
                        className="qb-linkish"
                        onClick={() => setOpen(r.id)}
                      >
                        {r.station_id}
                        {r.group ? ` · ${r.group}` : ""}
                      </button>
                    </td>
                    <td>
                      {/* ⛔ 没检验过画虚线框，不画 0 分 */}
                      {never ? (
                        <span className="qb-st-never">从没检验过</span>
                      ) : (
                        <span className="qb-st-cred">
                          <span className="bar">
                            <i
                              style={{
                                width: `${latest.round.trust}%`,
                                background:
                                  latest.round.trust >= 80
                                    ? "var(--ok)"
                                    : latest.round.trust >= 55
                                      ? "var(--warn)"
                                      : "var(--danger)",
                              }}
                            />
                          </span>
                          <b>{latest.round.trust}</b>
                          <EvidencePill level={latest.round.evidence} />
                        </span>
                      )}
                    </td>
                    <td>{never ? "—" : when(latest.round.at_ms)}</td>
                    <td>
                      {never ? (
                        "—"
                      ) : fails.length ? (
                        <span className="qb-st-fails">{fails.join(" · ")}</span>
                      ) : (
                        <span style={{ color: "var(--text-3)" }}>没有</span>
                      )}
                    </td>
                    <td>
                      {cell.kind === "differs" ? (
                        <>
                          <span className="qb-st-strike">
                            ×{cell.nominal.toFixed(2)}
                          </span>{" "}
                          <b style={{ color: "var(--danger)" }}>
                            ×{cell.real.toFixed(2)}
                          </b>
                        </>
                      ) : cell.kind === "verified" ? (
                        `×${cell.rate.toFixed(2)}`
                      ) : cell.nominal == null ? (
                        "—"
                      ) : (
                        `×${cell.nominal.toFixed(2)} 标称`
                      )}
                    </td>
                    <td className="num">
                      <Button variant="ghost" onClick={() => setOpen(r.id)}>
                        查看
                      </Button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>

      <div className="qb-st-runbar">
        <span>{chosen.length ? `已选 ${chosen.length} 条` : "还没选"}</span>
        <Button
          variant="primary"
          disabled={busy || chosen.length === 0}
          onClick={runAll}
        >
          开始检验
        </Button>
        <span style={{ flex: 1 }} />
        <span className="pill pill--warn">会计费 · 只在你点的时候跑</span>
      </div>
      <div className="qb-st-bound">
        只检测、只如实报告 —— 不拉黑、不替你换站、不上报。
      </div>
    </>
  );
}

function Report({
  route,
  rounds,
  onBack,
  onRun,
  busy,
}: {
  route?: Route;
  rounds: StoredAudit[];
  /** `null` = 没有清单可回（从账户行直接点进来的）。 */
  onBack: (() => void) | null;
  onRun: () => void;
  busy: boolean;
}) {
  const [idx, setIdx] = useState(0);
  const round = rounds[idx]?.round;
  const previous = rounds[idx + 1]?.round;

  return (
    <>
      {onBack && (
        <button className="qb-linkish" onClick={onBack}>
          ← 回清单
        </button>
      )}

      <div className="qb-card">
        <h3>
          {route
            ? `${route.station_id}${route.group ? ` · ${route.group}` : ""}`
            : "线路"}
        </h3>
        {!round ? (
          <>
            <p className="qb-st-note">
              这条线路<b>从没检验过</b>。可信度不是 0 分 —— 是还没有断言。
            </p>
            <Button variant="primary" disabled={busy} onClick={onRun}>
              跑一轮检验（会计费）
            </Button>
          </>
        ) : (
          <Verdict
            round={round}
            previous={previous}
            rounds={rounds}
            idx={idx}
            onPick={setIdx}
            onRun={onRun}
            busy={busy}
          />
        )}
      </div>
    </>
  );
}

/**
 * 判决书（四版里的 A 版，使用者选的）。
 *
 * # 为什么结论要顶在最上面
 *
 * 使用者打开这个弹窗只有一个问题:**这站还能不能用**。六张卡平铺开来,
 * 那个问题的答案得他自己从六个数里拼出来 —— 而拼出来的第一步恰恰是
 * 最容易拼错的:低分有两种完全相反的来源。
 *
 * ⛔ **29 分的两种来源在这里必须长得不一样。**
 *
 * | 来源 | 该做什么 | 这里怎么显示 |
 * |---|---|---|
 * | 六项都测了、几项对不上 | 换站 | 红底、写「这站有问题」 |
 * | 只测到一两项 | 再跑一轮 | 虚线灰底、写「还不能下结论」 |
 *
 * 只给一个分数的话，两者在界面上一模一样 —— 这正是 `evidence` 必须
 * 跟 `trust` 一起显示的理由（CLAUDE.md 里那一节）。
 *
 * # ⛔ 对不上的摊开，其余折起来
 *
 * 「没测到」不是「对不上」。把它们平铺在一起，五项没测到会看起来像
 * 五项都出了问题 —— 而实际上一项问题都没查出来。
 *
 * # ⛔ 查出造假也不拉黑
 *
 * 结论只有一个:把真实倍率算出来，拿真实值去排序。这一页没有任何
 * 「排除 / 禁用」的按钮，文案上也要说清楚这是使用者自己的决定。
 */
function Verdict({
  round,
  previous,
  rounds,
  idx,
  onPick,
  onRun,
  busy,
}: {
  round: StoredAudit["round"];
  previous?: StoredAudit["round"];
  rounds: StoredAudit[];
  idx: number;
  onPick: (i: number) => void;
  onRun: () => void;
  busy: boolean;
}) {
  const bad = round.checks.filter((c) => c.verdict === "differs");
  const ok = round.checks.filter((c) => c.verdict === "matches");
  const unk = round.checks.filter((c) => c.verdict === "unmeasured");
  // 「下得了结论」的门槛是证据，不是分数。⛔ 别拿 trust 当门槛 ——
  // 那会让「只测到一项、恰好对上」读成高分可信，而它什么都没证明。
  const conclusive =
    round.evidence === "sufficient" || round.evidence === "conflict";
  const total = round.checks.length;

  return (
    <>
      <div className={`qb-st-lead${conclusive ? " bad" : " unk"}`}>
        <TrustRing trust={round.trust} bad={conclusive} />
        <div>
          <h4>
            {conclusive
              ? bad.length
                ? `这站有问题 —— ${bad.length} 项对不上`
                : "六项都对得上"
              : "还不能下结论 —— 证据不足"}
          </h4>
          <p>
            {conclusive
              ? bad.length
                ? "六项都测到了，所以这个分数下得了结论。真实倍率已经按实测值参与排序和底线筛选；要不要继续用它是你的决定 —— 面板不替你拉黑。"
                : "六项都测到了，而且都对得上。"
              : `${total} 项里只测到 ${total - unk.length} 项。分数低是因为没测到的太多，不是因为这站被查出什么。该做的是再跑一轮，不是换站。`}
          </p>
          <div className="qb-st-leadpills">
            <EvidencePill level={round.evidence} />
            {round.mult == null ? (
              <span className="qb-st-pill qb-st-pill--unknown">
                这一轮没量到实扣倍率
              </span>
            ) : (
              <span className="qb-st-pill qb-st-pill--accent">
                实扣是标称的 ×{round.mult.toFixed(2)}
              </span>
            )}
            <span className="qb-st-pill qb-st-pill--unknown">
              {when(round.at_ms)}
            </span>
          </div>
        </div>
        <span style={{ flex: 1 }} />
        <Button variant="ghost" disabled={busy} onClick={onRun}>
          重新检验
        </Button>
      </div>

      {rounds.length > 1 && (
        <div className="qb-st-rounds">
          {rounds.map((r, i) => (
            <button
              key={r.round.at_ms.toString()}
              className="rd"
              aria-pressed={i === idx}
              onClick={() => onPick(i)}
            >
              <b>{r.round.trust}</b>
              <small>{when(r.round.at_ms)}</small>
            </button>
          ))}
        </div>
      )}

      {bad.length > 0 && (
        <>
          <div className="qb-st-sect">对不上的 {bad.length} 项</div>
          <div className="qb-st-checks">
            {bad.map((c) => (
              <CheckCard key={c.kind} check={c} previous={previous} />
            ))}
          </div>
        </>
      )}

      {ok.length + unk.length > 0 && (
        <details className="qb-st-adv">
          <summary>
            其余 {ok.length + unk.length} 项（{ok.length} 项相符、
            {unk.length} 项没测到）
          </summary>
          <div className="qb-st-checks">
            {[...ok, ...unk].map((c) => (
              <CheckCard key={c.kind} check={c} previous={previous} />
            ))}
          </div>
          <p className="qb-st-note">
            没测到的<b>不算「对不上」</b> —— 它只压低可信度。
          </p>
        </details>
      )}

      <div className="qb-st-sect">四类价目对照</div>
      <RateTable rates={round.rates} model={round.model} />
    </>
  );
}

/**
 * 可信度环。
 *
 * ⛔ 颜色跟着**证据**走，不跟着分数走 —— 同样是 29 分，证据不足那一档
 * 画成红色会让人以为查出了问题，而实际上一项问题都没查出来。
 */
function TrustRing({ trust, bad }: { trust: number; bad: boolean }) {
  const R = 36;
  const C = 2 * Math.PI * R;
  const on = (trust / 100) * C;
  return (
    <div className="qb-st-ring">
      <svg width="84" height="84" viewBox="0 0 84 84" aria-hidden="true">
        <circle
          cx="42"
          cy="42"
          r={R}
          fill="none"
          stroke="var(--surface-2)"
          strokeWidth={8}
        />
        <circle
          cx="42"
          cy="42"
          r={R}
          fill="none"
          stroke={bad ? "var(--danger)" : "var(--text-3)"}
          strokeWidth={8}
          strokeLinecap="round"
          strokeDasharray={`${on.toFixed(1)} ${(C - on).toFixed(1)}`}
        />
      </svg>
      <span className="mid">
        <b>{trust}</b>
        <span>可信度 / 100</span>
      </span>
    </div>
  );
}

/**
 * 四类价目对照。**只做展示，不下判定。**
 *
 * ⛔ 这张表回答的是「这家站点公布的计费结构长什么样」，不回答
 * 「它有没有超收」。后者只有上面那一格「倍率」的实测值答得了 ——
 * 那一个的分子是账单实扣、分母是同一批 token 按官方价算出来的成本，
 * 两头都不是站点自己说的数。
 *
 * ⛔ 空值一律写 `—`，不写 0。0 会被读成「这一类免费」。
 */
function RateTable({
  rates,
  model,
}: {
  rates: CategoryVerdict[];
  /** 这一轮验的是哪个模型。空串 = 老数据，不知道。 */
  model: string;
}) {
  if (rates.length === 0) return null;
  const kind = rates[0].basis_kind;
  const b = BASIS[kind];
  const absolute = kind === "absolute-prices";
  return (
    <div style={{ marginTop: 14 }}>
      <div
        style={{
          display: "flex",
          gap: 8,
          alignItems: "center",
          marginBottom: 6,
        }}
      >
        <span className={`pill ${b.tone}`}>{b.pill}</span>
        {/* ⛔ 模型名不能省：`官方单价 $5` 是哪个模型的 $5，不同模型差十倍。 */}
        <span className="qb-st-note">
          {model ? `这一轮验的是 ${model}` : "这一轮验的是哪个模型：没记下来"}
        </span>
      </div>
      <div className="qb-st-tw">
        <table>
          <thead>
            <tr>
              <th>类别</th>
              <th className="num">{b.head}</th>
              <th className="num">官方单价</th>
              <th className="num">{absolute ? "相对官方" : "官方价来源"}</th>
            </tr>
          </thead>
          <tbody>
            {rates.map((v) => {
              const mine = absolute ? v.station_price : v.station_ratio;
              const rel =
                absolute && v.station_price != null && v.official
                  ? v.station_price / v.official
                  : null;
              return (
                <tr key={v.category}>
                  <td>{CATEGORY[v.category]}</td>
                  <td className="num">
                    {mine == null
                      ? "—"
                      : absolute
                        ? `$${trim(mine)}`
                        : `×${trim(mine)}`}
                  </td>
                  <td className="num">
                    {v.official == null ? "—" : `$${trim(v.official)}`}
                  </td>
                  <td className="num">
                    {absolute
                      ? rel == null
                        ? "—"
                        : `×${trim(rel)}`
                      : v.basis == null
                        ? "—"
                        : v.basis === "published"
                          ? "官方标的"
                          : "按标准倍数推的"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <div className="qb-st-note">{b.note}</div>
    </div>
  );
}

/** 去掉尾零：2.50 → 2.5，0.100 → 0.1，25 → 25。 */
function trim(v: number): string {
  return Number(v.toFixed(4)).toString();
}

/**
 * 一项检查。**对不上的那几项画成「标称 → 实测」的对比**，其余画成读数。
 *
 * # ⛔ 三种行态必须一眼分得开
 *
 * | 判定 | 长什么样 | 为什么 |
 * |---|---|---|
 * | 对不上 | 红底 · 标称划掉 + 实测标红 | 这是这一页唯一要人看的东西 |
 * | 相符 | 实线框 · 一个读数 | 没有信息量，不该抢眼 |
 * | 没测到 | **虚线框** · 写「没测到」 | 它不是「对不上」，只压低可信度 |
 *
 * 把「没测到」画成跟「对不上」一样，五项没测到会看起来像五项都出了问题 ——
 * 而实际上一项问题都没查出来。
 */
function CheckCard({
  check,
  previous,
}: {
  check: AuditCheck;
  previous?: StoredAudit["round"];
}) {
  const prev = previous?.checks.find((p) => p.kind === check.kind)?.measured;
  const bad = check.verdict === "differs";
  const unk = check.verdict === "unmeasured";
  return (
    <div className={`qb-st-chk${bad ? " bad" : ""}${unk ? " unk" : ""}`}>
      <div className="hd">
        <b>{KIND[check.kind]}</b>
        <span
          className={`pill ${bad ? "pill--danger" : check.verdict === "matches" ? "pill--ok" : "pill--neutral"}`}
        >
          {check.verdict === "matches" ? "相符" : bad ? "对不上" : "没测到"}
        </span>
      </div>

      {bad ? (
        // 标称划掉 + 实测标红。⛔ 两个数摆在一起才看得出差多少 ——
        // 分行写的话人得自己做减法。
        <div className="delta">
          <s>{num(check.claimed, check.kind)}</s>
          <span className="arrow">→</span>
          <b>{num(check.measured, check.kind)}</b>
        </div>
      ) : (
        <div className="v">{num(check.measured, check.kind)}</div>
      )}

      <div className="s">
        {/* ⛔ 首次检验时不写 0 —— 那是个断言。 */}
        标称 {num(check.claimed, check.kind)} · 上次{" "}
        {prev == null ? (
          <span className="qb-st-dim">首次检验</span>
        ) : (
          num(prev, check.kind)
        )}
      </div>

      {unk && (
        <div className="qb-st-note">
          这一轮没量到 —— 它会压低可信度，但<b>不算「对不上」</b>。
        </div>
      )}
    </div>
  );
}

/**
 * 一个读数按它那一项该有的样子写。
 *
 * ⛔ **不能一律印原始数字。** `成功率 0.964` 和 `首字 1550` 是两种量纲，
 * 摆在一起人得自己换算 —— 而换错的方向恰好是「这条线看起来还行」。
 *
 * ⛔ 取不到写 `—`，**绝不写 0**：0 是量出来的断言，没量到不是。
 */
function num(v: number | null | undefined, kind?: CheckKind): React.ReactNode {
  if (v == null) return <span className="qb-st-dim">—</span>;
  switch (kind) {
    case "rate":
      return `×${Number(v.toFixed(3))}`;
    case "cache-hit":
    case "success-rate":
      return `${(v * 100).toFixed(1)}%`;
    case "context-window":
    case "max-output":
      return v >= 1000 ? `${(v / 1000).toFixed(v % 1000 ? 1 : 0)}K` : String(v);
    case "first-token":
      return `${(v / 1000).toFixed(2)} s`;
    default:
      return Number(v.toFixed(4)).toString();
  }
}

/**
 * `09-14 21:33`。
 *
 * ⛔ 不用 `toLocaleString()` —— 它在不同机器上给出完全不同的长度
 * （`9/14/2025, 9:33:20 PM` 对 `2025/9/14 21:33:20`），轮次那一排按钮
 * 会因此忽宽忽窄，而截图对比拿不准该不该红。
 */
function when(ms: bigint | number): string {
  const d = new Date(Number(ms));
  if (Number.isNaN(d.getTime())) return "—";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(
    d.getMinutes(),
  )}`;
}
