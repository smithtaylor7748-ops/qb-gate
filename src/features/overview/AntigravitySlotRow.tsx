/**
 * 反重力账户列表里的一行（0.32.0）。
 *
 * # 一行 = 一个 Google 账户，底下两半
 *
 * 0.30.0–0.31.0 是两个页签：「IDE 槽位」与「Gemini CLI 槽位」。同一个账户要建两次、
 * 登两次，而只用 IDE 的人永远看着一句「Gemini CLI · 0 个槽位」—— 它读起来像个故障，
 * 其实只是「你还没建」。现在两半在同一行上，各有一枚徽标、各有一个登录按钮。
 *
 * # ⛔ 三种状态要分得开
 *
 * | 徽标 | 意思 | 下一步 |
 * |---|---|---|
 * | 已登录 | 那一半有凭据 | —— |
 * | 未登录 | 目录建好了，还没登 | 点它去登 |
 * | 还没有这一半 | 连目录都没有（多半是从旧清单升上来的） | 点它现建再登 |
 *
 * 第三种是升级迁移的产物：老的两套清单**各自**升成一条「只填了一半」的行，
 * 面板**不猜配对**（IDE 的邮箱在状态库里，而 CLI 那边只看凭据文件在不在、从不读内容，
 * 没有任何依据把两者对上号）。要合由使用者自己点「并入…」。
 *
 * # 额度（2026-09-23 重做）
 *
 * 使用者对着截图说「Gemini 的余额显示不准」：这一行原来只画 IDE 写在本机的 `userStatus`，
 * 每个模型一个比例、只有 IDE 开着时才更新，那个槽位停在两天前。现在：
 *
 * | 有什么 | 画什么 |
 * |---|---|
 * | 点过右边的刷新（联网问到了四格） | Claude / Gemini 两行，每行 5h 与周两格 + 倒计时；来源行写问的时刻、档位、AI 积分 |
 * | 联网问到了、但是免费档（Google 不给四格） | 按模型合成的两组 |
 * | 还没点过 | IDE 写在本机的最低那一族，来源行写「IDE 写入 hh:mm」—— 不说成此刻 |
 *
 * ⛔ 联网**只从右边那颗刷新图标发出**，一次只问这一个账户（使用者定的：只手动刷新）。
 * `remaining === null` 是「没读到」，不是 0 —— 共用 `ui/Gauge` 的斜纹档。
 *
 * 原来行首还有一颗「打开 / 登录」，2026-09-23 使用者删了：起 IDE 走右边的启动磁贴，
 * 没登录时 IDE 徽标旁边本来就有「登录」。
 */
import { RefreshCw, Trash2 } from "lucide-react";

import { Button, Gauge, Pill } from "../../ui";
import { slotName } from "../../lib/slotName";
import {
  countdown,
  groupLabel,
  groupWindows,
  lowestQuota,
  modelGroups,
  percent,
  resetIn,
  spanLabel,
  tierShort,
} from "../../lib/antigravityQuota";
import type { AntigravityAccount } from "../../lib/generated/AntigravityAccount";
import type { AntigravityOnlineQuota } from "../../lib/generated/AntigravityOnlineQuota";
import type { AntigravityQuotaGroup } from "../../lib/generated/AntigravityQuotaGroup";
import type { AntigravityQuotaSpan } from "../../lib/generated/AntigravityQuotaSpan";
import type { AntigravityQuotaWindow } from "../../lib/generated/AntigravityQuotaWindow";

export interface SlotRowProps {
  slot: AntigravityAccount;
  busy: boolean;
  ideInstalled: boolean;
  cliInstalled: boolean;
  /** 此刻的时钟，给「几小时后重置」用。由父组件统一给，避免每行各走各的表。 */
  now: number;
  /** 正在挑「并入哪一条」时，源槽位的 id；`null` = 没在挑。 */
  attachFrom: string | null;
  /** 这个账户最近一次联网问到的额度。没问过就是 `null`。 */
  online: AntigravityOnlineQuota | null;
  /** 这一次刷新的错（留着上一次问到的，原因写在来源行）。 */
  onlineError: string;
  onlineLoading: boolean;
  /** 右边那颗刷新图标：联网问一次**这一个**账户。 */
  onRefresh: () => void;
  onSwitch: () => void;
  onOpenIde: () => void;
  onLoginCli: () => void;
  onArchive: () => void;
  /** 开始挑（传 `null` 取消）。 */
  onAttachStart: (id: string | null) => void;
  /** 把 `attachFrom` 那条并到这一行。 */
  onAttachHere: () => void;
}

/** 这一行缺的那一半，正好是 `from` 那一行有的吗。 */
export function canAttach(
  target: AntigravityAccount,
  from: AntigravityAccount,
): boolean {
  if (target.id === from.id) return false;
  const ideClash = !!target.ide_dir && !!from.ide_dir;
  const cliClash = !!target.cli_dir && !!from.cli_dir;
  return !ideClash && !cliClash;
}

/** `2026-09-23 07:01` → `09-23 07:01`：来源行窄，年份没人需要。 */
function shortStamp(value: string): string {
  return /^\d{4}-/.test(value) ? value.slice(5) : value;
}

/** 一半的徽标 + 它那颗按钮。 */
function Half({
  name,
  dir,
  loggedIn,
  state,
  action,
  disabled,
  onClick,
}: {
  name: string;
  dir: string | null;
  loggedIn: boolean;
  state: string;
  action: string;
  disabled: boolean;
  onClick: () => void;
}) {
  // 三种状态三种说法。把「还没有这一半」显示成「未登录」会让人去点一个
  // 其实不存在的登录 —— 而点完之后它才被建出来，说法和事实差了一步。
  // 第四种（2026-09-23）：点刷新时 Google 说刷新令牌作废了 —— 令牌还躺在本机，
  // 可登录已经没了。后端给的原因一律以「登录已失效」开头（`usecase::login_health`）。
  const rejected = !loggedIn && !!dir && state.startsWith("登录已失效");
  const tone = loggedIn ? "ok" : rejected ? "danger" : dir ? "warn" : "default";
  const text = loggedIn
    ? `${name} 已登录`
    : rejected
      ? `${name} 登录已失效`
      : dir
        ? `${name} 未登录`
        : `${name} ·`;
  return (
    <span className="ag-half" title={state}>
      <Pill tone={tone}>{text}</Pill>
      {!loggedIn && (
        <Button size="sm" variant="ghost" disabled={disabled} onClick={onClick}>
          {action}
        </Button>
      )}
    </span>
  );
}

/** 一格：「5h ███ 100% 4h55m」。缺这一格就画斜纹，**不画 0**。 */
function WindowGauge({
  w,
  span,
  now,
}: {
  w: AntigravityQuotaWindow | null;
  span: AntigravityQuotaSpan;
  now: number;
}) {
  const name = spanLabel(span);
  if (!w) return <Gauge className="gauge--mini" name={name} used={null} />;
  return (
    <Gauge
      className="gauge--mini"
      name={name}
      used={100 - w.remaining * 100}
      value={percent(w.remaining)}
      extra={
        w.reset_epoch != null ? (
          <span
            className="gauge-reset"
            title={[
              `重置于 ${w.reset_at ?? "（时刻读不出来）"}`,
              w.remaining_implied
                ? "Google 没给这一格的比例 —— 它的 JSON 会把 0 省掉，按用光算"
                : "",
              w.note ?? "",
            ]
              .filter(Boolean)
              .join(" · ")}
          >
            {countdown(w.reset_epoch, now)}
          </span>
        ) : undefined
      }
    />
  );
}

/** 一组一行：组名 + 5h + 周。 */
function GroupRow({
  group,
  windows,
  now,
}: {
  group: AntigravityQuotaGroup;
  windows: AntigravityQuotaWindow[];
  now: number;
}) {
  const [five, week] = groupWindows(windows, group);
  return (
    <div className="ag-group">
      <span
        className="ag-group-name"
        title={
          group === "claude"
            ? "Claude 与 GPT-OSS 共用这一份额度（Google 的分组）"
            : "Gemini 各模型共用这一份额度"
        }
      >
        {groupLabel(group)}
      </span>
      <WindowGauge w={five} span="five-hour" now={now} />
      <WindowGauge w={week} span="weekly" now={now} />
    </div>
  );
}

export default function AntigravitySlotRow({
  slot: s,
  busy,
  ideInstalled,
  cliInstalled,
  now,
  attachFrom,
  online,
  onlineError,
  onlineLoading,
  onRefresh,
  onSwitch,
  onOpenIde,
  onLoginCli,
  onArchive,
  onAttachStart,
  onAttachHere,
}: SlotRowProps) {
  // `lowestQuota` 自己会先分族，别在外面再分一次（分两遍的结果是 `label`/`model_id`
  // 在第二遍里已经没了）。
  const low = lowestQuota(s.quota);
  const picking = attachFrom === s.id;
  // 只填了一半的行才谈得上「并入」。两半都在的行没什么可并的。
  const halfEmpty = !s.ide_dir || !s.cli_dir;

  // ---------------------------------------------------------- 额度区
  const bars =
    online && online.windows.length > 0 ? (
      <>
        <GroupRow group="claude" windows={online.windows} now={now} />
        <GroupRow group="gemini" windows={online.windows} now={now} />
      </>
    ) : online && online.models.length > 0 ? (
      modelGroups(online.models).map(({ group, model }) => (
        <Gauge
          key={group}
          name={groupLabel(group)}
          used={model.remaining == null ? null : 100 - model.remaining * 100}
          value={
            model.remaining == null
              ? undefined
              : `剩 ${Math.round(model.remaining * 100)}%`
          }
          extra={
            <span
              className="gauge-reset"
              title={`${model.label} · 重置于 ${model.reset_at ?? "（时刻读不出来）"}`}
            >
              ↻ {resetIn(model.reset_epoch, now)}
            </span>
          }
        />
      ))
    ) : s.identity_error ? (
      // ⛔ 读不出来 ≠ 没登录。有 identity_error 时说的是前者。
      <span className="notice qb-tone-warn" title={s.identity_error}>
        账户状态读不出来：{s.identity_error}
      </span>
    ) : low ? (
      <Gauge
        name={low.family}
        used={low.remaining == null ? null : 100 - low.remaining * 100}
        value={
          low.remaining == null
            ? undefined
            : `剩 ${Math.round(low.remaining * 100)}%`
        }
        extra={
          <span className="gauge-reset" title="IDE 上次同步时写下的重置时刻">
            ↻ {resetIn(low.reset_epoch, now)}
          </span>
        }
      />
    ) : (
      <span className="notice">
        {s.ide_logged_in
          ? "IDE 还没写下额度信息"
          : s.ide_dir
            ? s.ide_auth_state
            : "还没有 IDE 那一半"}
      </span>
    );

  const tier = online ? tierShort(online.tier_id, online.tier_name) : "";
  const source: { text: string; title: string; warn: boolean } | null =
    onlineError
      ? { text: "刷新没成 · 悬停看原因", title: onlineError, warn: true }
      : online
        ? {
            text: [
              `在线 · ${shortStamp(online.fetched_at)}`,
              tier,
              online.windows.length === 0 && online.models.length > 0
                ? "按模型"
                : "",
              online.credits != null
                ? `AI 积分 ${online.credits.toLocaleString("zh-CN")}`
                : "",
            ]
              .filter(Boolean)
              .join(" · "),
            title: [
              `联网问于 ${online.fetched_at}`,
              online.tier_name ?? "",
              online.windows_note ?? "",
            ]
              .filter(Boolean)
              .join(" · "),
            warn: false,
          }
        : s.written_at && (low || s.quota.length > 0)
          ? {
              text: `IDE 写入 ${shortStamp(s.written_at)} · 点右边刷新联网查`,
              title:
                "这是 IDE 上次同步时写在本机的数，只有 IDE 开着时才更新；5 小时 / 每周两格要联网才有",
              warn: false,
            }
          : null;

  return (
    <div
      className={"slotrow" + (s.active ? " slotrow--active" : "")}
      data-testid="ag-slot"
    >
      <div className="slotrow-main">
        {/* 「邮箱 - 命名」，邮箱是 IDE 自己写在状态库里的。nowrap + 省略号，title 挂全名。 */}
        <strong className="slotrow-label" title={slotName(s.email, s.label)}>
          {slotName(s.email, s.label)}
        </strong>
        <span className="slotrow-side">
          {attachFrom && attachFrom !== s.id ? (
            <Button
              size="sm"
              variant="primary"
              disabled={busy}
              onClick={onAttachHere}
            >
              并到这里
            </Button>
          ) : picking ? (
            <Button
              size="sm"
              disabled={busy}
              onClick={() => onAttachStart(null)}
            >
              取消
            </Button>
          ) : (
            <>
              {s.active ? (
                <Pill tone="accent">当前账户</Pill>
              ) : (
                <Button size="sm" disabled={busy} onClick={onSwitch}>
                  切换
                </Button>
              )}
              <Button
                size="sm"
                aria-label={`移除反重力账户 ${slotName(s.email, s.label)}`}
                icon={<Trash2 size={12} />}
                disabled={busy || s.active}
                onClick={onArchive}
              />
            </>
          )}
        </span>
      </div>

      <div className="ag-halves">
        <Half
          name="IDE"
          dir={s.ide_dir}
          loggedIn={s.ide_logged_in}
          state={s.ide_auth_state}
          action="登录"
          disabled={busy || !ideInstalled}
          onClick={onOpenIde}
        />
        <Half
          name="CLI"
          dir={s.cli_dir}
          loggedIn={s.cli_logged_in}
          state={s.cli_auth_state}
          action={s.cli_dir ? "登录" : "建并登录"}
          disabled={busy || !cliInstalled}
          onClick={onLoginCli}
        />
        {halfEmpty && !attachFrom && (
          <Button
            size="sm"
            variant="ghost"
            disabled={busy}
            title="这一条只填了一半（多半是从旧清单升上来的）。认得出哪一条跟它是同一个 Google 账户，就并成一条 —— 只改索引，两边的目录都留在原地"
            onClick={() => onAttachStart(s.id)}
          >
            并入…
          </Button>
        )}
      </div>

      <div
        className="slotusage slotusage--refreshable ag-slotusage"
        data-testid="ag-slot-quota"
      >
        <div className="slotusage-bars">{bars}</div>
        <Button
          size="sm"
          variant="ghost"
          className="slotusage-refresh"
          icon={<RefreshCw size={12} />}
          aria-label={`联网刷新 ${slotName(s.email, s.label)} 的额度`}
          title={
            s.ide_logged_in
              ? "联网问一次这个账户的额度（Google 官方接口，只问这一个）"
              : "IDE 那一半还没登录，没有令牌可问"
          }
          loading={onlineLoading}
          disabled={!s.ide_logged_in}
          onClick={onRefresh}
        />
        {source && (
          <span
            className={`gauge-src${source.warn ? " qb-tone-warn" : ""}`}
            title={source.title}
          >
            {source.text}
          </span>
        )}
      </div>
    </div>
  );
}
