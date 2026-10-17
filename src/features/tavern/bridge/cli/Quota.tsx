/**
 * GPT / Gemini 桥接设置里的联网额度卡（原来 `GptQuota` 与 `GeminiQuota` 两份几乎一样的组件，2026-09-29 并成一份）。
 *
 * ⛔ **只手动刷新**（2026-09-23 使用者定的）：展开桥接设置只读「最近一次」（`refresh: false`，后端绝不联网），
 * 点「刷新」才联网，而且只问这一家。
 * ⛔ Google 没给比例的那一格按 0 算、带「推算」（它的 JSON 会把 0 省掉）—— 悬停里说出来。
 */
import { useCallback, useEffect, useState } from "react";

import { api } from "../../../../lib/api";
import { Button, Gauge as QuotaBar } from "../../../../ui";
import { errorText } from "../../card/common";
import { resetLabel, type CliProvider } from "./meta";

interface Row {
  key: string;
  name: string;
  remaining: number | null;
  resetAt: string | null;
  resetEpoch: number | null;
  implied: boolean;
}

interface Snapshot {
  meta: string | null;
  fetchedAt: string | null;
  rows: Row[];
}

async function read(
  provider: CliProvider,
  refresh: boolean,
): Promise<Snapshot | null> {
  if (provider === "gpt") {
    const q = await api.tavernGptQuota(refresh);
    if (!q) return null;
    return {
      meta: q.account,
      fetchedAt: q.fetched_at || null,
      rows: q.windows.map((w, i) => ({
        key: `${w.label}#${i}`,
        name: w.label,
        remaining: w.remaining_percent,
        resetAt: w.reset_at,
        resetEpoch: w.reset_epoch,
        implied: false,
      })),
    };
  }
  const q = await api.tavernGeminiQuota(refresh);
  if (!q) return null;
  return {
    meta: q.tier ? `${q.tier}${q.project ? ` · ${q.project}` : ""}` : null,
    fetchedAt: q.fetched_at || null,
    rows: q.models.map((m, i) => ({
      // 汇总接口的桶没有 model_id / token_type（2026-09-25 起不再被合成一个），键要带上名字与序号。
      key: `${m.model_id}-${m.token_type}-${m.label}#${i}`,
      name: m.label,
      remaining: m.remaining_percent,
      resetAt: m.reset_at,
      resetEpoch: m.reset_epoch,
      implied: m.remaining_implied,
    })),
  };
}

/** 30 秒走一次的钟：「几分后重置」不该停在打开弹窗那一刻。 */
function useClock() {
  const [clock, setClock] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setClock(Date.now()), 30_000);
    return () => window.clearInterval(timer);
  }, []);
  return clock;
}

export default function Quota({
  provider,
  id,
}: {
  provider: CliProvider;
  id: string;
}) {
  const now = useClock();
  const [snap, setSnap] = useState<Snapshot | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(
    async (refresh: boolean) => {
      setBusy(true);
      setError(null);
      try {
        const q = await read(provider, refresh);
        if (q || refresh) setSnap(q);
      } catch (e) {
        setError(errorText(e));
      } finally {
        setBusy(false);
      }
    },
    [provider],
  );
  useEffect(() => {
    void load(false);
  }, [load]);

  const gpt = provider === "gpt";
  const rows = snap?.rows ?? [];
  return (
    <div
      id={id}
      className={`tavern-quota-card tavern-quota-card--${provider}`}
      data-testid={`tavern-${provider}-quota`}
    >
      <div className="tavern-quota-head">
        <strong>
          {gpt ? "ChatGPT / Codex 剩余额度" : "Google Gemini CLI 模型额度"}
        </strong>
        <Button size="sm" loading={busy} onClick={() => void load(true)}>
          刷新
        </Button>
      </div>
      {snap?.meta && <span className="tavern-quota-meta">{snap.meta}</span>}
      {error && <p className="tavern-quota-error">{error}</p>}
      <div className={gpt ? "tavern-quota-grid" : "tavern-quota-models"}>
        {rows.map((r) => (
          <div key={r.key} className="tavern-quota-window">
            <QuotaBar
              name={r.name}
              used={r.remaining == null ? null : 100 - r.remaining}
              value={r.remaining == null ? "剩余未知" : `剩 ${r.remaining}%`}
              extra={
                <span
                  className="gauge-reset"
                  title={
                    r.implied
                      ? "Google 没给这一格的比例 —— 它的 JSON 会把 0 省掉，按用光算"
                      : undefined
                  }
                >
                  ↻ {r.resetAt ?? "重置时间未知"} ·{" "}
                  {resetLabel(r.resetEpoch, now)}
                  {r.implied && <em>推算</em>}
                </span>
              }
              className="tavern-quota-gauge"
            />
          </div>
        ))}
      </div>
      {rows.length === 0 && (
        <p className="tavern-quota-empty">
          {gpt
            ? "还没联网问过；点「刷新」问一次（只在你点的时候问）。"
            : "还没联网问过；登录 CLI 后点「刷新」问一次（只在你点的时候问）。"}
        </p>
      )}
      <small className="tavern-quota-meta">
        {snap?.fetchedAt ? `读取于 ${snap.fetchedAt}` : "尚未读取额度"}
      </small>
    </div>
  );
}
