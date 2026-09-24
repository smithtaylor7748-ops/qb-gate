/**
 * 一个账户槽位的详情。全局只挂一份，谁要看都喊 `requestAccountDetail(label)`。
 *
 * 跟 `SecuritySheet` 同一个套路：会话态存一个 label，这里读它。
 *
 * # 这里补上了四个「合同上有、界面上从来没画过」的字段
 *
 * `account_uuid`、`plan_fetched_at`、`usage.measured_at`，以及 0.20.0 新加的
 * 邮箱 / 组织名 / 到期时刻。其中 `plan_fetched_at` 的类型注释自己写着
 * 「界面上要标出来 —— 这是缓存，可能过期」，而在这之前没有任何地方画它。
 *
 * # ⛔ 「重新登录」必须常驻，不能只在过期时出现
 *
 * 剩余天数只读本地那个时间戳，**查不出「被风控下线」**。令牌被回收时
 * `.credentials.json` 还在、到期日还是将来某一天，看起来完全健康，
 * 而实际已经发不出请求了 —— 那正是使用者点名要这个按钮的场景。
 * 按「看起来过期没」决定要不要给这个入口，等于在最需要它的时候把它藏起来。
 *
 * # token 统计只覆盖这个槽位目录里的会话
 *
 * 没经过面板、直接用官方默认目录 `~\.claude` 跑的不在内。
 * 这句话必须画在界面上，否则使用者会把这个数字当成「这个账户一共用了多少」。
 */

import { useMemo, useState } from "react";
import { FolderOpen, KeyRound, LogIn, Stethoscope, Trash2 } from "lucide-react";

import {
  api,
  type AccountProbe,
  type AccountProbeState,
  type Slot,
  type TokenUsage,
} from "../../lib/api";
import { R } from "../../lib/resources";
import { res, useResource, useSession } from "../../lib/store";
import { slotName } from "../../lib/slotName";
import { Button, Modal, Pill, fmtDaysLeft } from "../../ui";
import { daysTone } from "../../features/overview/SlotRow";
import SlotUsageBars from "../../features/overview/SlotUsage";
import {
  DETAIL_KEY,
  requestDelete,
  requestSwitch,
  requestSwitchAndLogin,
} from "./requests";

/** 时间范围。`0` = 全部。 */
const RANGES = [
  [7, "近 7 天"],
  [30, "近 30 天"],
  [0, "全部"],
] as const;

const NUM = new Intl.NumberFormat("zh-CN");

/** 一行「名字：值」。值读不出来时写「读不出来」，不留空。 */
function Fact({
  k,
  v,
  hint,
}: {
  k: string;
  v: React.ReactNode;
  hint?: string;
}) {
  // 形状照 `.qb-facts > div > span`（`workspace.css`）：第一个 span 是名字，
  // strong 是值。hint 再来一个 span，拿的是同一条淡色小字的样式。
  return (
    <div>
      <span>{k}</span>
      <strong>{v ?? "读不出来"}</strong>
      {hint && <span>{hint}</span>}
    </div>
  );
}

export default function AccountDetail() {
  const [label, setLabel] = useSession<string | null>(DETAIL_KEY, null);
  const accounts = useResource("accounts", R.accounts);
  const slot = accounts.data?.slots.find((s) => s.label === label) ?? null;

  function close() {
    setLabel(null);
  }

  return (
    <Modal
      open={!!label && !!slot}
      onClose={close}
      title={`账户 ${slot ? slotName(slot.email, slot.label) : (label ?? "")}`}
      size="huge"
      footer={<Button onClick={close}>关闭</Button>}
    >
      {slot && <Body slot={slot} onClose={close} />}
    </Modal>
  );
}

/**
 * 「还能用吗」的实测。
 *
 * `EXPIRY_CAVEAT` 一直写着「唯一能确认的办法是实际发一次认证请求」，
 * 而在 0.20.0 之前面板从来没发过 —— 剩余天数看起来健康、账户其实已经
 * 被回收，界面上一个字都看不出来。这个按钮补的就是那一半。
 *
 * ⛔ 它会**带着你的官方令牌**对外发一次请求。所以按钮上必须写明这一点，
 * 不许做成一个看起来纯本地的「检查」。
 */
function ProbeRow({ label }: { label: string }) {
  const [busy, setBusy] = useState(false);
  const [r, setR] = useState<AccountProbe | null>(null);

  const TONE: Record<AccountProbeState, "ok" | "warn" | "danger" | "default"> =
    {
      accepted: "ok",
      rejected: "danger",
      locally_expired: "warn",
      no_credential: "default",
      unreachable: "default",
    };
  const WORD: Record<AccountProbeState, string> = {
    accepted: "服务端认",
    rejected: "服务端拒了",
    locally_expired: "本地令牌已过期",
    no_credential: "没登录过",
    unreachable: "测不出来",
  };

  async function run() {
    setBusy(true);
    try {
      setR(await api.accountProbe(label));
    } catch (e) {
      setR({
        state: "unreachable",
        detail: e instanceof Error ? e.message : String(e),
        checked_at: "",
      });
    } finally {
      setBusy(false);
    }
  }

  return (
    <section>
      <div className="mb-1 flex flex-wrap items-center gap-2">
        <h3 className="card-title">
          <Stethoscope size={14} aria-hidden="true" /> 还能用吗
        </h3>
        {r && <Pill tone={TONE[r.state]}>{WORD[r.state]}</Pill>}
        {r?.checked_at && <span className="notice">{r.checked_at} 测的</span>}
        <span className="ml-auto">
          <Button
            size="sm"
            variant="primary"
            loading={busy}
            onClick={() => void run()}
          >
            实测一次
          </Button>
        </span>
      </div>
      <p className="notice">
        {r
          ? r.detail
          : "剩余天数只读本地时间戳，查不出令牌是不是已经被回收。这一测会带着这个账户的令牌向官方发一次最小请求（公开的模型列表端点）——不查额度、不打模型、不往槽位里写任何东西。"}
      </p>
    </section>
  );
}

function Body({ slot: s, onClose }: { slot: Slot; onClose: () => void }) {
  const accounts = useResource("accounts", R.accounts);

  return (
    <div className="flex flex-col gap-4">
      {/* -------------------------------------------------- 动作 */}
      <div className="flex flex-wrap items-center gap-2">
        <Pill tone={s.active ? "accent" : "default"}>
          {s.active ? "当前账户" : "未使用"}
        </Pill>
        <Pill tone={daysTone(s)}>
          {s.logged_in ? fmtDaysLeft(s.cli_days_left) : "未登录"}
        </Pill>
        <span className="ml-auto flex flex-wrap gap-2">
          {!s.active && (
            <Button size="sm" onClick={() => requestSwitch(s.label)}>
              切换到这个账户
            </Button>
          )}
          {/* 常驻，不按「看起来过期没」决定 —— 见文件头。

              ⛔ **对当前账户也走一遍完整的「关全部 + 切换 + 起会话」，
              这是使用者定的，不是漏改。** 槽位横条上那个「重新登录」只
              `launch("claude-code")`，不清场；这里故意更重：
              重登要处理的正是「令牌被回收 / 状态不对」，而那种时候
              内存里那份旧凭证还在别的进程手上握着，不清场就等于
              带着半截旧状态再登一次。使用者的原话：「就要这种非常保守的」。
              有人来「优化」掉这一下之前，先去看 `requestSwitchAndLogin`
              的注释和这一条。 */}
          <Button
            size="sm"
            variant="primary"
            icon={<LogIn size={12} />}
            onClick={() => {
              onClose();
              requestSwitchAndLogin(s.label);
            }}
          >
            重新登录
          </Button>
          <Button
            size="sm"
            variant="danger"
            icon={<Trash2 size={12} />}
            disabled={s.active}
            title={
              s.active
                ? "当前账户删不了：删掉它会留下一个指向空处的 claude-profile，而没有任何东西会去修它。请先切换到另一个账户。"
                : undefined
            }
            onClick={() => {
              onClose();
              requestDelete(s.label);
            }}
          >
            删除
          </Button>
        </span>
      </div>

      <p className="notice">
        「重新登录」会先切过去（关掉全部 Claude）再起一个 Claude Code。
        没到期也能用 —— 剩余天数查不出令牌是不是已经被风控回收。
      </p>

      <ProbeRow label={s.label} />

      {/* -------------------------------------------------- 身份 */}
      <section>
        <h3 className="card-title">身份</h3>
        <div className="qb-facts qb-facts--wide">
          <Fact k="槽位名" v={s.label} />
          <Fact k="登录邮箱" v={s.email} />
          <Fact k="组织" v={s.org_name} />
          <Fact
            k="账户 UUID"
            v={s.account_uuid && <code>{s.account_uuid}</code>}
          />
        </div>
      </section>

      {/* -------------------------------------------------- 套餐与凭证 */}
      <section>
        <h3 className="card-title">套餐与凭证</h3>
        <div className="qb-facts qb-facts--wide">
          <Fact k="套餐" v={s.plan} />
          <Fact k="计费方式" v={s.billing} />
          <Fact
            k="档案缓存时间"
            v={s.plan_fetched_at}
            hint="官方客户端上次刷新这份档案的时刻。这是缓存，可能已经旧了。"
          />
          <Fact k="本地登录记录" v={s.logged_in ? "有" : "没有"} />
          <Fact
            k="凭证到期"
            v={
              s.logged_in
                ? (s.expires_at ?? "读不出到期时间")
                : "还没在这个槽位里登录过"
            }
            hint={s.logged_in ? fmtDaysLeft(s.cli_days_left) : undefined}
          />
        </div>
        <p className="notice mt-2">{accounts.data?.planCaveat}</p>
        <p className="notice">{accounts.data?.caveat}</p>
      </section>

      {/* -------------------------------------------------- 额度 */}
      <section>
        <h3 className="card-title">额度窗口</h3>
        {s.usage ? (
          <>
            <SlotUsageBars usage={s.usage} />
            <p className="notice mt-2">
              测于 {s.usage.measured_at}。桌面端约 15 分钟一条样本，没开时不写。
            </p>
          </>
        ) : (
          <p className="notice">两个来源都没有这个槽位的读数。</p>
        )}
      </section>

      {/* -------------------------------------------------- token */}
      <Tokens label={s.label} />

      {/* -------------------------------------------------- 目录 */}
      <section>
        <h3 className="card-title">
          <FolderOpen size={14} aria-hidden="true" /> 目录
        </h3>
        <div className="qb-facts qb-facts--wide">
          <Fact k="槽位目录" v={<code>{s.dir}</code>} />
          <Fact
            k="桌面端资料"
            v={
              s.desktop_dir ? <code>{s.desktop_dir}</code> : "读不出 %APPDATA%"
            }
            hint={
              s.desktop_profile
                ? "这个槽位有自己的桌面端资料。"
                : "这个槽位还没有独立的桌面端资料，桌面端现在用的是别处那份。"
            }
          />
        </div>
      </section>
    </div>
  );
}

// ------------------------------------------------------------ token 统计

function Tokens({ label }: { label: string }) {
  // 一个槽位一个 queryKey。`res` 对象每次 render 新建一个是没关系的
  // （`useResource` 只是把它记进 definitions），但 `useMemo` 能少建几个。
  const def = useMemo(
    () => res(() => api.accountsTokens(label), { auto: false }),
    [label],
  );
  const tokens = useResource(`tokens:${label}`, def);

  const [days, setDays] = useState<number>(30);
  const [model, setModel] = useState<string>("");

  const data: TokenUsage | undefined = tokens.data;
  const models = useMemo(
    () => [...new Set((data?.buckets ?? []).map((b) => b.model))].sort(),
    [data],
  );

  const since = useMemo(() => {
    if (!days) return "";
    const d = new Date();
    d.setDate(d.getDate() - (days - 1));
    // 桶的 day 是本地日期串，这里也用本地日期串比，不经过 UTC。
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
      d.getDate(),
    ).padStart(2, "0")}`;
  }, [days]);

  const rows = useMemo(() => {
    const picked = (data?.buckets ?? []).filter(
      (b) => (!since || b.day >= since) && (!model || b.model === model),
    );
    // 按天汇总（同一天多个模型合成一行），倒序 —— 最近的在上面。
    const byDay = new Map<string, (typeof picked)[number]>();
    for (const b of picked) {
      const at = byDay.get(b.day);
      if (!at) {
        byDay.set(b.day, { ...b, model: model || "全部模型" });
        continue;
      }
      at.input += b.input;
      at.output += b.output;
      at.cache_write += b.cache_write;
      at.cache_read += b.cache_read;
      at.messages += b.messages;
    }
    return [...byDay.values()].sort((a, b) => b.day.localeCompare(a.day));
  }, [data, since, model]);

  const total = rows.reduce(
    (acc, r) => ({
      input: acc.input + r.input,
      output: acc.output + r.output,
      cache_write: acc.cache_write + r.cache_write,
      cache_read: acc.cache_read + r.cache_read,
      messages: acc.messages + r.messages,
    }),
    { input: 0, output: 0, cache_write: 0, cache_read: 0, messages: 0 },
  );

  return (
    <section>
      <div className="mb-1 flex flex-wrap items-center gap-2">
        <h3 className="card-title">
          <KeyRound size={14} aria-hidden="true" /> 用掉的 token
        </h3>
        <span className="ml-auto flex flex-wrap gap-1.5">
          {RANGES.map(([d, name]) => (
            <Button
              key={d}
              size="sm"
              variant={days === d ? "primary" : "default"}
              onClick={() => setDays(d)}
              disabled={!data}
            >
              {name}
            </Button>
          ))}
          <Button
            size="sm"
            loading={tokens.loading}
            onClick={() => void tokens.refresh()}
          >
            {data ? "重新统计" : "统计"}
          </Button>
        </span>
      </div>

      {tokens.error && <p className="notice notice--danger">{tokens.error}</p>}

      {!data ? (
        <p className="notice">
          读一遍这个槽位的会话转写。<strong>零网络请求</strong>。
        </p>
      ) : (
        <>
          {models.length > 1 && (
            <div className="mb-2 flex flex-wrap gap-1.5">
              <Button
                size="sm"
                variant={model === "" ? "primary" : "default"}
                onClick={() => setModel("")}
              >
                全部模型
              </Button>
              {models.map((m) => (
                <Button
                  key={m}
                  size="sm"
                  variant={model === m ? "primary" : "default"}
                  onClick={() => setModel(m)}
                >
                  {m}
                </Button>
              ))}
            </div>
          )}

          <div className="qb-acct-tw">
            <table>
              <thead>
                <tr>
                  <th>日期</th>
                  <th className="num">输入</th>
                  <th className="num">输出</th>
                  <th className="num">缓存写</th>
                  <th className="num">缓存读</th>
                  <th className="num">回复数</th>
                </tr>
              </thead>
              <tbody>
                {rows.length === 0 ? (
                  <tr>
                    <td colSpan={6}>这个范围里没有记录。</td>
                  </tr>
                ) : (
                  rows.map((r) => (
                    <tr key={r.day}>
                      <td>{r.day}</td>
                      <td className="num">{NUM.format(r.input)}</td>
                      <td className="num">{NUM.format(r.output)}</td>
                      <td className="num">{NUM.format(r.cache_write)}</td>
                      <td className="num">{NUM.format(r.cache_read)}</td>
                      <td className="num">{NUM.format(r.messages)}</td>
                    </tr>
                  ))
                )}
              </tbody>
              {rows.length > 0 && (
                <tfoot>
                  <tr>
                    <th>合计</th>
                    <th className="num">{NUM.format(total.input)}</th>
                    <th className="num">{NUM.format(total.output)}</th>
                    <th className="num">{NUM.format(total.cache_write)}</th>
                    <th className="num">{NUM.format(total.cache_read)}</th>
                    <th className="num">{NUM.format(total.messages)}</th>
                  </tr>
                </tfoot>
              )}
            </table>
          </div>

          <p className="notice mt-2">
            只算这个槽位目录里的 {data.sessions} 个会话（{data.files_read}{" "}
            个转写文件
            {data.files_failed > 0 && `，${data.files_failed} 个读不出来`}
            ，去掉 {NUM.format(data.duplicates)} 条重放
            {data.undated > 0 && `，${data.undated} 条没带时刻`}）。 用默认目录{" "}
            <code>~\.claude</code> 跑的不在内，也不折算成钱。
          </p>
        </>
      )}
    </section>
  );
}
