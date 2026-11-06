/**
 * Claude 账户列表里的一行（2026-11-06 重新设计，使用者：「感觉现在的太臃肿了」）。
 *
 * 一行三块，从左到右：账户（邮箱、命名、套餐、剩余天数）· 额度（5 小时 / 7 天两根条）· 操作（图标按钮）。
 * 没有表头 —— 每一块自己看得懂，省下的那一行留给账户。三个账户页共用同一套列宽（`workspace.css` 的 `.acct-row`）。
 * 原来是一个带框的盒子，里面四五层（标题行、两根条、来源、刷新图标）。
 * ⛔ 这一页不照 Cockpit Tools 的账户页做（使用者更正过：照 Cockpit 的只有会话弹窗），怎么排是这里自己定的。
 *
 * # 「切换」这个词对过期槽位是错的
 *
 * `logged_in` 只是「`.credentials.json` 在不在」，`cli_days_left` 是
 * refreshToken 剩余天数，两者故意不合并 —— **过期槽位必须仍然可切**，
 * 因为你得先切过去才能在那个槽里重新登录（档案 §4.8，
 * `expired_slot_is_still_switchable` 那个单测钉着）。
 * 所以不禁用按钮，只换文案（图标按钮的可访问名与悬停提示）。
 *
 * # 打开详情的是邮箱，不是整行
 *
 * 整行可点听起来更顺手，但这一行里本来就有按钮 —— 嵌套的可交互元素
 * 对键盘和读屏都是个坑（点哪儿算哪个？Tab 停几次？）。
 * 邮箱本身做成按钮，位置固定、读屏报得出名字，右边的动作各归各的。
 */

import { RefreshCw } from "lucide-react";
import type { Slot } from "../../lib/api";
import { Button, Pill, fmtDaysLeft } from "../../ui";
import SlotUsageBars from "./SlotUsage";

/**
 * 剩余天数的语气。
 *
 * `null` 是「读不出到期时间」，**不是「过期」**，所以给中性 ——
 * 读不出来就报警，等于把「不知道」说成「坏了」。
 */
export function daysTone(s: Slot) {
  if (!s.logged_in) return "default" as const;
  const d = s.cli_days_left;
  if (d == null) return "default" as const;
  if (d < 0) return "danger" as const;
  if (d < 5) return "warn" as const;
  return "ok" as const;
}

/** 按钮上该写什么。过期与没登过是两种话，合并了就看不出该做什么。 */
export function switchLabel(s: Slot): string {
  if (!s.logged_in) return "切换并登录";
  if ((s.cli_days_left ?? 0) < 0) return "切换并重登";
  return "切换";
}

/**
 * 到期那一格的文字。服务端明确拒绝了这份登录（点刷新时问出来的，`login_problem`）
 * 跟「从没登过」分开说 —— 前者是「登录已失效」，后者才是「未登录」。
 */
export function expiryText(s: Slot): string {
  // 访问令牌被拒只是那一张令牌不认了（Claude Code 跑起来会自己换），不等于要重新登录。
  if (s.login_problem?.startsWith("访问令牌被拒")) return "令牌被拒";
  if (s.login_problem) return "登录已失效";
  return s.logged_in ? fmtDaysLeft(s.cli_days_left) : "未登录";
}

export default function SlotRow({
  slot: s,
  onOpen,
  actions,
  badges,
  onRefresh,
  refreshing,
}: {
  slot: Slot;
  /** 点邮箱打开详情。不给就是纯展示。 */
  onOpen?: (label: string) => void;
  /** 操作列（刷新图标之后）。各个页面自己决定放什么。 */
  actions?: React.ReactNode;
  /** 账户列第二行末尾的小徽标（凭证有事时、超额之类）。 */
  badges?: React.ReactNode;
  /** 操作列第一颗：向官方问一次这个账户的额度（阶段 6 · U2）。不给就不画。 */
  onRefresh?: () => void;
  refreshing?: boolean;
}) {
  // 邮箱是主名，命名放第二行；没读到邮箱就拿命名顶上来。nowrap + 省略号，title 挂全名。
  const main = s.email ?? s.label;
  const tone = daysTone(s);
  const bad = !!s.login_problem || !s.logged_in;
  return (
    <div className={`slotrow acct-row${s.active ? " slotrow--active" : ""}`}>
      <div className="acct-who">
        <div className="acct-line">
          {onOpen ? (
            <button
              type="button"
              className="acct-email slotrow-open"
              title={`查看 ${main} 的详情`}
              onClick={() => onOpen(s.label)}
            >
              {main}
            </button>
          ) : (
            <span className="acct-email" title={main}>
              {main}
            </span>
          )}
          {s.active && <Pill tone="ok">当前</Pill>}
        </div>
        <div className="acct-line acct-meta">
          {s.email && s.email !== s.label && (
            <span className="acct-name" title={s.label}>
              {s.label}
            </span>
          )}
          <span className="acct-plan" title={s.billing ?? undefined}>
            {s.plan ?? "套餐未知"}
          </span>
          {/* 剩余天数只在有事时上色：正常的那一档是灰字，不抢眼。 */}
          <span
            className={`acct-days${
              bad
                ? " qb-tone-danger"
                : tone === "warn" || tone === "danger"
                  ? ` qb-tone-${tone}`
                  : ""
            }`}
            title={s.login_problem ?? undefined}
          >
            {expiryText(s)}
          </span>
          {badges}
        </div>
      </div>
      <div className="acct-quota">
        {/* 额度。两源都没有就不画条 —— 给一个「剩 100%」比不给更糟，
            那是替一个根本没读到的数字打包票。 */}
        {s.usage || (onRefresh && s.logged_in) ? (
          <SlotUsageBars usage={s.usage ?? null} />
        ) : (
          <span className="gauge-src">—</span>
        )}
      </div>
      <div className="acct-ops">
        {onRefresh && (
          <Button
            size="sm"
            variant="ghost"
            icon={<RefreshCw size={13} />}
            aria-label="联网刷新这个账户的额度"
            title="用这个账户的令牌向官方问一次额度（只读，一次只问这一个）"
            loading={refreshing}
            onClick={onRefresh}
          />
        )}
        {actions}
      </div>
    </div>
  );
}
