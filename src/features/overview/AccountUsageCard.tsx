/**
 * 当前账户的用量小结：用了多少 token、缓存命中多少、缓存省下多少钱。
 *
 * # ⛔ 这里的美元是「缓存省下的」，不是「你花了多少」
 *
 * 订阅账户按官方 API 单价折出来的总额**不是你实际付的钱**（你付的是月费）。
 * 所以这里只给一个说得清口径的数：这些缓存读的 token 如果不走缓存、
 * 按整价重新读一遍要多花多少。算式与取价在 Rust 侧
 * （`usecase::token_summary`），认不出价的模型直接跳过并报数，
 * **不拿别的模型的价去凑** —— 凑出来的数字看起来跟真的一模一样。
 *
 * # 范围
 *
 * 只数当前槽位目录里的会话转写。没经过面板、直接用官方默认目录跑的不在内。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { Gauge, RefreshCw } from "lucide-react";

import { api, type TokenSummary } from "../../lib/api";
import { R } from "../../lib/resources";
import { useResource } from "../../lib/store";
import { Button, Card, Modal } from "../../ui";

/** 时间档。`0` = 全部。 */
const RANGES = [
  [1, "今天"],
  [7, "7 天"],
  [0, "全部"],
] as const;

/**
 * 自己重算的间隔。
 *
 * 0.20.0 之前这张卡的 `useEffect` 只依赖 `[label, days]` —— 两个都是不变的值，
 * 于是「今天用了多少」从开面板那一刻起就**冻住**，跑一下午 Claude 也不动。
 * 一个叫「今天」的数字不会自己往前走，是这张卡最容易让人误信的地方。
 *
 * 60 秒：这一轮要扫本机的会话转写（实测两个目录合计约 90 MB，Rust 侧
 * 一遍在百毫秒量级），比账户列表那条 30 秒的贵得多，所以放慢一倍。
 */
const REFRESH_MS = 60_000;

const NUM = new Intl.NumberFormat("zh-CN");

/** 大数字缩写：1.2M / 34.5K。表格里要精确值，这里要一眼看得懂。 */
function short(n: number): string {
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return NUM.format(n);
}

function Stat({
  name,
  value,
  sub,
}: {
  name: string;
  value: string;
  sub?: string;
}) {
  return (
    <div className="ustat">
      <span className="ustat-name">{name}</span>
      <span className="ustat-value">{value}</span>
      {sub && <span className="ustat-sub">{sub}</span>}
    </div>
  );
}

export default function AccountUsageCard() {
  const [details, setDetails] = useState(false);
  const accounts = useResource("accounts", R.accounts);
  const label = accounts.data?.slots.find((s) => s.active)?.label ?? "";

  const [days, setDays] = useState<number>(1);
  const [data, setData] = useState<TokenSummary | null>(null);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);
  const [at, setAt] = useState<Date | null>(null);

  const load = useCallback(
    async (signal?: { cancelled: boolean }) => {
      if (!label) return;
      setBusy(true);
      setErr("");
      try {
        const r = await api.accountsTokenSummary(label, days);
        if (!signal?.cancelled) {
          setData(r);
          setAt(new Date());
        }
      } catch (e) {
        if (!signal?.cancelled)
          setErr(e instanceof Error ? e.message : String(e));
      } finally {
        if (!signal?.cancelled) setBusy(false);
      }
    },
    [label, days],
  );

  useEffect(() => {
    if (!label) {
      setData(null);
      setAt(null);
      return;
    }
    setData(null);
    setAt(null);
    const signal = { cancelled: false };
    void load(signal);
    // ⛔ 这个定时器是这张卡的正确性的一部分，不是装饰 —— 见 `REFRESH_MS`。
    const timer = window.setInterval(() => void load(signal), REFRESH_MS);
    return () => {
      signal.cancelled = true;
      window.clearInterval(timer);
    };
  }, [label, load]);

  const hit = useMemo(
    () =>
      data?.hit_rate == null ? null : `${Math.round(data.hit_rate * 100)}%`,
    [data],
  );

  if (!label)
    return (
      <Card className="account-usage" title="当前账户的用量">
        <p className="notice">选择并登录账户后显示本机用量。</p>
      </Card>
    );

  return (
    <Card className="account-usage">
      <div className="usage-heading">
        <h2 className="card-title" title={`${label} 的用量`}>
          <Gauge size={14} aria-hidden="true" />
          {label} 的用量
        </h2>
        <span className="usage-ranges">
          {RANGES.map(([d, name]) => (
            <Button
              key={d}
              size="sm"
              variant={days === d ? "primary" : "default"}
              disabled={busy}
              onClick={() => setDays(d)}
            >
              {name}
            </Button>
          ))}
          <Button
            size="sm"
            icon={<RefreshCw size={12} />}
            loading={busy}
            aria-label="立刻重算用量"
            title="立刻重算。后台每分钟也会自己读一次。"
            onClick={() => void load()}
          />
        </span>
      </div>

      {err && <p className="notice notice--danger">{err}</p>}

      {/* ⛔ 头条不放「总 token」。实测这台机器上缓存读占到 96%
          （3.23 亿里 3.11 亿），把四类加起来当「用了多少」，
          读到的是一个被缓存读淹掉的数字 —— 输入只有 6 千多。
          所以四格是「输入 / 输出 / 缓存 / 省下」，总量降级成副标。 */}
      <div className="ustats">
        <Stat
          name="输入"
          value={data ? short(data.input) : "—"}
          sub={data ? `${NUM.format(data.messages)} 条回复` : undefined}
        />
        <Stat
          name="输出"
          value={data ? short(data.output) : "—"}
          sub={
            data
              ? `四类合计 ${short(
                  data.input + data.output + data.cache_read + data.cache_write,
                )}`
              : undefined
          }
        />
        <Stat
          name="缓存命中率"
          value={hit ?? "—"}
          sub={
            data
              ? `读 ${short(data.cache_read)} · 写 ${short(data.cache_write)}`
              : undefined
          }
        />
        <Stat
          name="缓存省下"
          value={
            data?.saved_usd == null ? "—" : `$${data.saved_usd.toFixed(2)}`
          }
          sub={
            data?.unpriced_models
              ? `${data.unpriced_models} 个模型没有官方价，未计入`
              : data?.priced_from_snapshot
                ? "按内置官方价快照算"
                : "按本次抓到的官方价算"
          }
        />
      </div>

      {/* 归不到槽位的那部分单独说。实测今天的记录**一条都没有归属标记**，
          所以这一行经常比账户自己的数还大 —— 瞒下来的话，使用者看到
          「今天 0」只会以为统计坏了。 */}
      <div className="usage-footer">
        <span className="notice">
          {data?.unattributed ? "有未归属记录" : "本机记录 · 每分钟刷新"}
        </span>
        <Button size="sm" variant="ghost" onClick={() => setDetails(true)}>
          统计说明
        </Button>
      </div>
      <Modal
        open={details}
        onClose={() => setDetails(false)}
        title={`${label} 的用量明细`}
      >
        <p className="notice">
          {at ? `${at.toTimeString().slice(0, 5)} 更新 · ` : ""}
          每分钟刷新。只统计当前账户的本机会话。
        </p>
        {data && (
          <p className="notice mt-2">
            输入 {NUM.format(data.input)} · 输出 {NUM.format(data.output)} ·
            缓存读 {NUM.format(data.cache_read)} · 缓存写{" "}
            {NUM.format(data.cache_write)} · {NUM.format(data.messages)}{" "}
            条回复。
          </p>
        )}
        {!!data?.unattributed && (
          <p className="notice mt-2">
            另有 <strong>{short(data.unattributed)}</strong> token（
            {NUM.format(data.unattributed_messages)} 条回复）
            <strong>归不到任何槽位</strong>，未计入本账户。
          </p>
        )}
        <p className="notice mt-2">
          缓存省下按官方 API 价估算，不是订阅实付费用。
        </p>
        <p className="notice mt-2">
          {data?.unpriced_models
            ? `${data.unpriced_models} 个模型没有官方价，未计入估算。`
            : data?.priced_from_snapshot
              ? "价格来自内置官方价快照。"
              : "价格来自本次读取的官方价格。"}
        </p>
      </Modal>
    </Card>
  );
}
