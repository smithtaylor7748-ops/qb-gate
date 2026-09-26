/**
 * 当前账户的用量小结：按官方 API 价折算的美元、回复数、token、缓存命中。
 *
 * # 美元：按官方 API 价折算，订阅不按这个收费（2026-09-24 使用者定的）
 *
 * 0.25.1 之前这张卡只给「缓存省下多少」，理由是订阅账户按 API 单价折出来的总额
 * 不是你实际付的钱。使用者 09-24 要的是「每天、7 天、30 天花了多少刀的额度」——
 * 跟 cc-switch 同一个口径。算式与取价在 Rust 侧（`usecase::token_summary` 文件头），
 * 认不出价的模型直接跳过并报数，**不拿别的模型的价去凑**。
 * 「非账单」这句写在页脚，跟数字在同一张卡上。
 *
 * # 2026-09-24 那张截图：「1 条回复」，输入输出全是 0
 *
 * 那一条是 `<synthetic>` 报错（「请重新登录」这类，四类 token 全 0），被当成了回复；
 * 什么都没计价时副标还写着「按本次抓到的官方价算」。现在报错不算回复（Rust 侧不入账、
 * 单独数在 `empty_replies`），副标只说真的发生了的事。
 *
 * # 范围
 *
 * 槽位目录里的会话转写，加上默认目录（`~\.claude`）里**归得出属**的那部分 ——
 * 桌面端 Code 页跑的会话全落在默认目录，靠桌面端自己留下的会话记录归属
 * （三级判定见 Rust 侧 `tokens.rs` 文件头）。三级都归不出的单独报「未归属」，
 * 不摊给任何账户。切过账户的那天，量会分在两个账户上 —— 用量明细页有「按账户」表。
 */

import { useCallback, useEffect, useState } from "react";
import { Gauge, RefreshCw } from "lucide-react";
import { Link } from "react-router-dom";

import { api, type TokenSummary } from "../../lib/api";
import { R } from "../../lib/resources";
import { useResource } from "../../lib/store";
import { slotName } from "../../lib/slotName";
import { today as todayYmd } from "../../lib/dates";
import { Button, Card } from "../../ui";

/** 时间档。跟使用者要的一样：今天 / 7 天 / 30 天。「全部」在用量明细页上。 */
const RANGES = [
  [1, "今天"],
  [7, "7 天"],
  [30, "30 天"],
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
const USD = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

/** 大数字缩写：1.2M / 34.5K。表格里要精确值，这里要一眼看得懂。 */
function short(n: number): string {
  if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + "B";
  if (n >= 1_000_000) return (n / 1_000_000).toFixed(1) + "M";
  if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
  return NUM.format(n);
}

function usd(v: number | null | undefined): string {
  if (v == null) return "—";
  if (v > 0 && v < 0.01) return "<$0.01";
  return USD.format(v);
}

/** `2026-09-24 02:39:00` → 今天的写 `02:39`，别的日子写 `09-23 02:39`。 */
function when(at: string): string {
  const [day, time] = at.split(" ");
  const hm = (time ?? "").slice(0, 5);
  return day === todayYmd() ? hm : `${day.slice(5)} ${hm}`;
}

function Stat({
  name,
  value,
  sub,
  title,
}: {
  name: string;
  value: string;
  sub?: string;
  title?: string;
}) {
  return (
    <div className="ustat" title={title}>
      <span className="ustat-name">{name}</span>
      <span className="ustat-value">{value}</span>
      {sub && <span className="ustat-sub">{sub}</span>}
    </div>
  );
}

/** 美元那一格下面那行：换一档看另一个时间窗，三档都在一次请求里（`spend`）。 */
function spendSub(data: TokenSummary): string {
  if (data.messages > 0 && data.cost_usd == null) return "这些模型都没有官方价";
  if (data.days === 1) return `近 7 天 ${usd(data.spend.last_7d.usd)}`;
  if (data.days === 7) return `近 30 天 ${usd(data.spend.last_30d.usd)}`;
  return `今天 ${usd(data.spend.today.usd)}`;
}

export default function AccountUsageCard() {
  const accounts = useResource("accounts", R.accounts);
  const activeSlot = accounts.data?.slots.find((s) => s.active);
  // label 是**标识**（拿去查用量的那个键），shown 是给人看的「邮箱 - 命名」。
  // 两者不许混用：把 shown 传给后端就查不到任何东西。
  const label = activeSlot?.label ?? "";
  const shown = slotName(activeSlot?.email, label);

  const [days, setDays] = useState<number>(1);
  const [data, setData] = useState<TokenSummary | null>(null);
  const [err, setErr] = useState("");
  const [busy, setBusy] = useState(false);

  const load = useCallback(
    async (signal?: { cancelled: boolean }) => {
      if (!label) return;
      setBusy(true);
      setErr("");
      try {
        const r = await api.accountsTokenSummary(label, days);
        if (!signal?.cancelled) {
          setData(r);
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
      return;
    }
    setData(null);
    const signal = { cancelled: false };
    void load(signal);
    // ⛔ 这个定时器是这张卡的正确性的一部分，不是装饰 —— 见 `REFRESH_MS`。
    const timer = window.setInterval(() => void load(signal), REFRESH_MS);
    return () => {
      signal.cancelled = true;
      window.clearInterval(timer);
    };
  }, [label, load]);

  if (!label)
    return (
      <Card className="account-usage" title="当前账户的用量">
        <p className="notice">选择并登录账户后显示本机用量。</p>
      </Card>
    );

  const hit =
    data?.hit_rate == null ? "—" : `${Math.round(data.hit_rate * 100)}%`;

  return (
    <Card className="account-usage">
      <div className="usage-heading">
        <h2 className="card-title" title={`${shown} 的用量`}>
          <Gauge size={14} aria-hidden="true" />
          {shown} 的用量
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

      {/* ⛔ 头条不放「总 token」。实测这台机器上缓存读占到 96%，把四类加起来当
          「用了多少」，读到的是一个被缓存读淹掉的数字。第一格是折算的美元 ——
          它已经把四类各按各的价加权过了，才是「用了多少」的那个数。 */}
      <div className="ustats">
        <Stat
          name="等价费用"
          value={data ? usd(data.cost_usd) : "—"}
          sub={data ? spendSub(data) : undefined}
          title="同样的 token 走官方 API 要付的钱。订阅账户付的是月费，不按这个收费。"
        />
        <Stat
          name="回复"
          value={data ? NUM.format(data.messages) : "—"}
          sub={
            data
              ? data.empty_replies > 0
                ? `另有 ${NUM.format(data.empty_replies)} 条报错未计`
                : data.last_at
                  ? `最后一条 ${when(data.last_at)}`
                  : "这段时间没有记录"
              : undefined
          }
        />
        <Stat
          name="输出"
          value={data ? short(data.output) : "—"}
          sub={
            data
              ? `输入 ${short(data.input)} · 缓存写 ${short(data.cache_write)}`
              : undefined
          }
        />
        <Stat
          name="缓存命中率"
          value={hit}
          sub={data ? `缓存读 ${short(data.cache_read)}` : undefined}
        />
      </div>

      {/* 归不到槽位的那部分单独说。瞒下来的话，使用者看到「今天 0」只会以为统计坏了。 */}
      <div className="usage-footer">
        <span className="notice">
          {/* 没算进美元的、没读成的都要说出来（文件头那句「跳过并报数」，2026-09-25 补上）：
              原来只在「全部都没有官方价」时才提，一部分没算进去时卡上只剩一个偏小的数。 */}
          {(() => {
            const caveats = [
              data?.unattributed
                ? `另有 ${NUM.format(data.unattributed_messages)} 条未归属`
                : "",
              data && data.unpriced_models > 0
                ? `${data.unpriced_models} 个模型没有官方价、没算进美元`
                : "",
              data?.coverage && data.coverage.files_failed > 0
                ? `${data.coverage.files_failed} 份记录没读成、统计不完整`
                : "",
            ].filter(Boolean);
            return caveats.length
              ? `${caveats.join(" · ")} · 美元按 API 价折算，非账单`
              : "美元按官方 API 价折算，非账单 · 每分钟刷新";
          })()}
        </span>
        {/* 0.32.0：明细搬到可滚动的 `/usage` 上去了 ——
            这一页是固定高度的，趋势图与按模型分布摆不下。 */}
        <Link to="/usage?side=claude" className="btn btn--sm btn--ghost">
          用量明细
        </Link>
      </div>
    </Card>
  );
}
