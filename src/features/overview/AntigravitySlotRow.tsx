import type { ReactNode } from "react";
/**
 * 反重力账户列表里的一行（0.32.0；2026-11-06 照 Cockpit Tools 的账户页重排成表格行）。
 *
 * 三列跟 Claude / GPT 两页同一套：账户 | 额度 | 操作。
 *
 * # 一行 = 一个 Google 账户，底下两半
 *
 * 0.30.0–0.31.0 是两个页签：「IDE 槽位」与「Gemini CLI 槽位」。同一个账户要建两次、
 * 登两次，而只用 IDE 的人永远看着一句「Gemini CLI · 0 个槽位」—— 它读起来像个故障，
 * 其实只是「你还没建」。现在两半在同一行上，各一枚小徽标；没登录的那一枚自己就是按钮，点它去登
 * （原来徽标旁边再摆一颗「登录」/「建并登录」，一行五六样东西）。
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
 * 没有任何依据把两者对上号）。要合由使用者自己点操作列的「并入…」。
 *
 * # 额度（2026-09-23 重做；2026-11-06 收成一张小矩阵）
 *
 * 使用者说原来那块「太乱」：Claude / Gemini 两行，每行都写一遍「5h」「周」，再加倒计时。
 * 现在「5h」「周」只在表头出现一次，一格只剩条子和百分比，倒计时放悬停：
 *
 * | 有什么 | 画什么 |
 * |---|---|
 * | 点过刷新（联网问到了四格） | Claude / Gemini 两行 × 5h / 周两格；来源行写问的时刻、档位、AI 积分 |
 * | 联网问到了、但是免费档（Google 不给 5 小时那一格） | 5h 那一格画斜纹（没读到，不是 0） |
 * | 联网问到了、但只有按模型的 | 按模型合成的两组 |
 * | 还没点过 | IDE 写在本机的最低那一族，来源行写「IDE 写入 hh:mm」—— 不说成此刻 |
 *
 * ⛔ 联网**只从这一行的刷新图标发出**，一次只问这一个账户（使用者定的：只手动刷新）；
 * Gemini CLI 那一半登着的话同一下也问它（原来来源行末尾单独一颗「CLI」，2026-11-06 去掉），
 * 结果写在来源行的悬停提示里。`remaining === null` 是「没读到」，不是 0 —— 共用 `ui/Gauge` 的斜纹档。
 */
import { ArrowLeftRight, GitMerge, RefreshCw, Trash2, X } from "lucide-react";

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
  tierShort,
} from "../../lib/antigravityQuota";
import type { AntigravityAccount } from "../../lib/generated/AntigravityAccount";
import type { AntigravityOnlineQuota } from "../../lib/generated/AntigravityOnlineQuota";
import type { AntigravityQuotaGroup } from "../../lib/generated/AntigravityQuotaGroup";
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
  /** 刷新图标：联网问一次**这一个**账户（CLI 那一半登着的话也问它）。 */
  onRefresh: () => void;
  onSwitch: () => void;
  onOpenIde: () => void;
  onLoginCli: () => void;
  onArchive: () => void;
  /** 开始挑（传 `null` 取消）。 */
  onAttachStart: (id: string | null) => void;
  /** 把 `attachFrom` 那条并到这一行。 */
  onAttachHere: () => void;
  /** 凭证徽标（`CredentialBadge`），由父组件给。 */
  credential?: ReactNode;
  /** Gemini CLI 那一半最近一次问到的额度摘要（悬停用）；`undefined` = 没问过。 */
  cliSummary?: string;
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

/** 一半的小徽标。没登录（且读得出来）时它自己就是按钮：点它去登 / 现建再登。 */
function Half({
  name,
  dir,
  loggedIn,
  unreadable,
  state,
  disabled,
  onClick,
}: {
  name: string;
  dir: string | null;
  loggedIn: boolean;
  /** 读不出来（状态库正忙 / 打不开、凭据文件读失败）。**不是「未登录」**（§7.17）。 */
  unreadable: boolean;
  state: string;
  disabled: boolean;
  onClick: () => void;
}) {
  // 三种状态三种说法。把「还没有这一半」显示成「未登录」会让人去点一个
  // 其实不存在的登录 —— 而点完之后它才被建出来，说法和事实差了一步。
  // 第四种（2026-09-23）：点刷新时 Google 说刷新令牌作废了 —— 令牌还躺在本机，
  // 可登录已经没了。后端给的原因一律以「登录已失效」开头（`usecase::login_health`）。
  // 第五种（2026-09-25）：读不出来。原来跟「未登录」一个样子 —— IDE 写库时占着锁，
  // 15 秒一轮的读就把一个登着的账户画成「未登录」、摆一颗「登录」。
  const rejected =
    !loggedIn &&
    !!dir &&
    (state.startsWith("登录已失效") || state.startsWith("已停用"));
  const cantRead = !loggedIn && !!dir && unreadable;
  const tone = loggedIn
    ? "ok"
    : rejected
      ? "danger"
      : cantRead || dir
        ? "warn"
        : "default";
  const text = loggedIn
    ? `${name} 已登录`
    : rejected
      ? `${name} 登录已失效`
      : cantRead
        ? `${name} 读不出来`
        : dir
          ? `${name} 未登录`
          : `${name} ·`;
  if (loggedIn || cantRead) {
    return (
      <span className="ag-half" title={state}>
        <Pill tone={tone}>{text}</Pill>
      </span>
    );
  }
  // 「建并登录」：没有 IDE 那一半时起 IDE 会先现建这一半（后端 `account::ensure_ide_dir`，2026-09-25）。
  const action = dir ? "登录" : "建并登录";
  return (
    <button
      type="button"
      className={`ag-half ag-half--btn pill pill--${tone}`}
      title={`${state} · 点一下${action}`}
      aria-label={`${text}，${action}`}
      disabled={disabled}
      onClick={onClick}
    >
      {text}
    </button>
  );
}

/** 一格：条子 + 百分比，倒计时在悬停里。缺这一格就画斜纹，**不画 0**。 */
function Cell({
  w,
  name,
  now,
}: {
  w: AntigravityQuotaWindow | null;
  name: string;
  now: number;
}) {
  if (!w)
    return (
      <Gauge
        className="gauge--cell"
        name={name}
        used={null}
        hint="这一格没有读到（免费档 Google 不给 5 小时那一格）"
      />
    );
  // 过了重置时刻就不画旧百分比。
  if (w.reset_epoch != null && w.reset_epoch * 1000 <= now)
    return (
      <Gauge
        className="gauge--cell"
        name={name}
        used={null}
        value="已重置"
        hint="过了重置时刻，这是重置前问到的数 · 点刷新再问一次"
      />
    );
  return (
    <Gauge
      className="gauge--cell"
      name={name}
      used={100 - w.remaining * 100}
      value={
        <>
          {percent(w.remaining)}
          {/* 推出来的 0 看得见，不只藏在悬停里（2026-09-25）。 */}
          {w.remaining_implied && <em className="ag-implied">推算</em>}
        </>
      }
      hint={[
        w.reset_epoch != null
          ? `还有 ${countdown(w.reset_epoch, now)} 重置（${w.reset_at ?? "时刻读不出来"}）`
          : "重置时刻读不出来",
        w.remaining_implied
          ? "Google 没给这一格的比例 —— 它的 JSON 会把 0 省掉，按用光算"
          : "",
        w.note ?? "",
      ]
        .filter(Boolean)
        .join(" · ")}
    />
  );
}

/** 一组一行：组名 + 5h + 周（Argon 只有周那一格）。 */
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
    <div className="ag-mrow">
      <span
        className="ag-mname"
        title={
          group === "claude"
            ? "Claude 与 GPT-OSS 共用这一份额度（Google 的分组）"
            : group === "argon"
              ? "Gemini 4 Argon 单独的一份周额度"
              : "Gemini 各模型共用这一份额度"
        }
      >
        {group === "argon" ? "Argon" : groupLabel(group)}
      </span>
      {group === "argon" ? (
        <span className="ag-mgap" aria-hidden="true" />
      ) : (
        <Cell w={five} name="5h" now={now} />
      )}
      <Cell w={week} name="周" now={now} />
    </div>
  );
}

/** 反重力那一页的表头：额度那一列分成「5h」「周」两小列，跟每行的小矩阵对齐。 */
export function AntigravityTableHead() {
  return (
    <div className="acct-head" aria-hidden="true">
      <span>账户</span>
      <span className="ag-mrow ag-mrow--head">
        <span>额度</span>
        <span>5h</span>
        <span>周</span>
      </span>
      <span>操作</span>
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
  credential,
  cliSummary,
}: SlotRowProps) {
  // `lowestQuota` 自己会先分族，别在外面再分一次（分两遍的结果是 `label`/`model_id`
  // 在第二遍里已经没了）。
  const low = lowestQuota(s.quota);
  const picking = attachFrom === s.id;
  // 只填了一半的行才谈得上「并入」。两半都在的行没什么可并的。
  const halfEmpty = !s.ide_dir || !s.cli_dir;
  const shown = slotName(s.email, s.label);

  // ---------------------------------------------------------- 额度区
  const bars =
    online && online.windows.length > 0 ? (
      <div className="ag-matrix">
        <GroupRow group="claude" windows={online.windows} now={now} />
        <GroupRow group="gemini" windows={online.windows} now={now} />
        {online.windows.some((w) => w.group === "argon") && (
          <GroupRow group="argon" windows={online.windows} now={now} />
        )}
      </div>
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
          hint={[
            `${model.label} · ${resetIn(model.reset_epoch, now) || "重置时刻读不出来"}${model.reset_at ? `（${model.reset_at}）` : ""}`,
            model.remaining_implied
              ? "Google 没给这个模型的比例 —— 它的 JSON 会把 0 省掉，按用光算"
              : "",
          ]
            .filter(Boolean)
            .join(" · ")}
        />
      ))
    ) : s.identity_error ? (
      // ⛔ 读不出来 ≠ 没登录。有 identity_error 时说的是前者。
      <span className="gauge-src qb-tone-warn" title={s.identity_error}>
        账户状态读不出来
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
        hint={`IDE 上次同步时写下的数${low.reset_epoch != null ? ` · ${resetIn(low.reset_epoch, now)}` : ""}`}
      />
    ) : (
      <span className="gauge-src">
        {s.ide_logged_in
          ? "IDE 还没写下额度信息"
          : s.ide_dir
            ? s.ide_auth_state
            : "还没有 IDE 那一半"}
      </span>
    );

  const tier = online ? tierShort(online.tier_id, online.tier_name) : "";
  // 联网问到了、但四格与按模型两样都是空的：上面画的是 IDE 写在本机的旧数，
  // 来源行不许照样写「在线 · 几点」—— 原来两天前的「剩 100%」就这么挂在「在线」底下（2026-09-25）。
  const onlineHasData =
    !!online && (online.windows.length > 0 || online.models.length > 0);
  const cliLine = cliSummary ? `Gemini CLI：${cliSummary}` : "";
  const source: { text: string; title: string; warn: boolean } | null =
    onlineError
      ? {
          text: "刷新没成 · 悬停看原因",
          title: [onlineError, cliLine].filter(Boolean).join(" · "),
          warn: true,
        }
      : online && !onlineHasData
        ? {
            text: `在线问到了，但没有额度数据${
              low && s.written_at
                ? ` · 条子是 IDE 写入 ${shortStamp(s.written_at)} 的`
                : ""
            }`,
            title: [
              `联网问于 ${online.fetched_at}`,
              online.windows_note ?? "",
              cliLine,
            ]
              .filter(Boolean)
              .join(" · "),
            warn: true,
          }
        : online
          ? {
              text: [
                online.last_success
                  ? `上次成功：${new Date(online.last_success).toLocaleString()}`
                  : `在线 · ${shortStamp(online.fetched_at)}`,
                tier,
                online.gcp_tos ? "GCP 条款" : "",
                online.windows.length === 0 && online.models.length > 0
                  ? "按模型"
                  : "",
                // 「可用 AI 积分」常驻；读不到写「—」，不补 0。
                `AI 积分 ${online.credits != null ? online.credits.toLocaleString("zh-CN") : "—"}`,
              ]
                .filter(Boolean)
                .join(" · "),
              title: [
                `联网问于 ${online.fetched_at}`,
                online.tier_name ?? "",
                online.windows_note ?? "",
                cliLine,
              ]
                .filter(Boolean)
                .join(" · "),
              warn: false,
            }
          : s.written_at && (low || s.quota.length > 0)
            ? {
                text: `IDE 写入 ${shortStamp(s.written_at)} · 点刷新联网查`,
                title: [
                  "这是 IDE 上次同步时写在本机的数，只有 IDE 开着时才更新；5 小时 / 每周两格要联网才有",
                  cliLine,
                ]
                  .filter(Boolean)
                  .join(" · "),
                warn: false,
              }
            : cliLine
              ? { text: "", title: cliLine, warn: false }
              : null;

  return (
    <div
      className={"slotrow acct-row" + (s.active ? " slotrow--active" : "")}
      data-testid="ag-slot"
    >
      <div className="acct-who">
        <div className="acct-line">
          {/* 邮箱是 IDE 自己写在状态库里的；没读到就拿命名顶上来。nowrap + 省略号，title 挂全名。 */}
          <strong className="acct-email" title={shown}>
            {s.email ?? s.label}
          </strong>
          {s.active && <Pill tone="ok">当前</Pill>}
        </div>
        <div className="acct-line acct-meta ag-halves">
          {s.email && (
            <span className="acct-name" title={s.label}>
              {s.label}
            </span>
          )}
          <Half
            name="IDE"
            dir={s.ide_dir}
            loggedIn={s.ide_logged_in}
            unreadable={s.ide_unreadable}
            state={s.ide_auth_state}
            disabled={busy || !ideInstalled}
            onClick={onOpenIde}
          />
          <Half
            name="CLI"
            dir={s.cli_dir}
            loggedIn={s.cli_logged_in}
            unreadable={s.cli_unreadable}
            state={s.cli_auth_state}
            disabled={busy || !cliInstalled}
            onClick={onLoginCli}
          />
          {credential}
        </div>
      </div>

      <div className="acct-quota" data-testid="ag-slot-quota">
        <div className="slotusage ag-slotusage">
          <div className="slotusage-bars">{bars}</div>
          {source?.text ? (
            <span
              className={`gauge-src${source.warn ? " qb-tone-warn" : ""}`}
              title={source.title}
            >
              {source.text}
            </span>
          ) : null}
        </div>
      </div>

      <div className="acct-ops">
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
            icon={<X size={13} />}
            disabled={busy}
            onClick={() => onAttachStart(null)}
          >
            取消
          </Button>
        ) : (
          <>
            <Button
              size="sm"
              variant="ghost"
              icon={<RefreshCw size={13} />}
              aria-label={`联网刷新 ${shown} 的额度`}
              title={
                s.ide_logged_in || s.ide_unreadable
                  ? `联网问一次这个账户的额度（Google 官方接口，只问这一个${s.cli_logged_in ? "；Gemini CLI 那一半也问" : ""}）${source?.title ? `\n${source.title}` : ""}`
                  : "IDE 那一半还没登录，没有令牌可问"
              }
              loading={onlineLoading}
              // 读不出来 ≠ 没登录：刷新照样给，真去问时读不读得出来由那一次说（2026-09-25）。
              disabled={!s.ide_logged_in && !s.ide_unreadable}
              onClick={onRefresh}
            />
            {halfEmpty && (
              <Button
                size="sm"
                variant="ghost"
                icon={<GitMerge size={13} />}
                aria-label="并入…"
                disabled={busy}
                title="这一条只填了一半（多半是从旧清单升上来的）。认得出哪一条跟它是同一个 Google 账户，就并成一条 —— 只改索引，两边的目录都留在原地"
                onClick={() => onAttachStart(s.id)}
              />
            )}
            {!s.active && (
              <Button
                size="sm"
                variant="ghost"
                icon={<ArrowLeftRight size={13} />}
                aria-label="切换"
                title={`切换：把当前账户换成 ${shown}`}
                disabled={busy}
                onClick={onSwitch}
              />
            )}
            <Button
              size="sm"
              variant="ghost"
              className="acct-del"
              aria-label={`移除反重力账户 ${shown}`}
              title={
                s.active
                  ? "当前账户移除不了：请先切换到另一个账户。"
                  : `移除 ${shown}（目录保留在本机归档里）`
              }
              icon={<Trash2 size={13} />}
              disabled={busy || s.active}
              onClick={onArchive}
            />
          </>
        )}
      </div>
    </div>
  );
}
