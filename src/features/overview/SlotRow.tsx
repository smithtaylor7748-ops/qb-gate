/**
 * 一条账户槽位横条。
 *
 * # 为什么把它抽出来
 *
 * 0.19.2 之前这一坨是 `AccountBand` 里 `pageSlots.map()` 中的一个裸 `div`。
 * 于是「管理」页（`Official.tsx`）自己另画了一套，显示的东西**比它要管的
 * 这一条还少** —— 没有剩余天数、没有额度、没有到期语气色。
 * 同一个东西两处各画一份，落后的那一份不会报错，只会悄悄变成另一个页面。
 *
 * 现在总览与详情页头部共用这一个（0.20.0 把那个「管理」页整个删了 ——
 * 使用者的原话：管理账户不就是删除吗）。
 *
 * # 「切换」这个词对过期槽位是错的
 *
 * `logged_in` 只是「`.credentials.json` 在不在」，`cli_days_left` 是
 * refreshToken 剩余天数，两者故意不合并 —— **过期槽位必须仍然可切**，
 * 因为你得先切过去才能在那个槽里重新登录（档案 §4.8，
 * `expired_slot_is_still_switchable` 那个单测钉着）。
 * 所以不禁用按钮，只换文案。
 *
 * # 打开详情的是标签，不是整行
 *
 * 整行可点听起来更顺手，但这一行里本来就有按钮 —— 嵌套的可交互元素
 * 对键盘和读屏都是个坑（点哪儿算哪个？Tab 停几次？）。
 * 标签本身做成按钮，位置固定、读屏报得出名字，右边的动作各归各的。
 */

import type { Slot } from "../../lib/api";
import { slotName } from "../../lib/slotName";
import { Pill, fmtDaysLeft } from "../../ui";
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

/** 到期那一格的文字。 */
export function expiryText(s: Slot): string {
  return s.logged_in ? fmtDaysLeft(s.cli_days_left) : "未登录";
}

export default function SlotRow({
  slot: s,
  onOpen,
  actions,
  onRefresh,
  refreshing,
}: {
  slot: Slot;
  /** 点标签打开详情。不给就是纯展示。 */
  onOpen?: (label: string) => void;
  /** 右侧动作区。各个页面自己决定放什么。 */
  actions?: React.ReactNode;
  /** 额度条右边的刷新图标（2026-09-23）。不给就不画。Claude 这一侧只重读本机，不联网。 */
  onRefresh?: () => void;
  refreshing?: boolean;
}) {
  // 「邮箱 - 命名」（0.27.0）。邮箱比命名长得多，而这一格是 nowrap + 省略号，
  // 所以 title 上挂完整的那一串 —— 省略号吃掉的部分要有地方看得到。
  const shown = slotName(s.email, s.label);
  return (
    <div className={`slotrow${s.active ? " slotrow--active" : ""}`}>
      <div className="slotrow-main">
        {onOpen ? (
          <button
            type="button"
            className="slotrow-label slotrow-open"
            title={`查看 ${shown} 的详情`}
            onClick={() => onOpen(s.label)}
          >
            {shown}
          </button>
        ) : (
          <span className="slotrow-label" title={shown}>
            {shown}
          </span>
        )}
        <span className="slotrow-plan" title={s.billing ?? undefined}>
          {s.plan ?? "套餐未知"}
        </span>
        <span className="slotrow-side">
          <Pill tone={daysTone(s)}>{expiryText(s)}</Pill>
          {actions}
        </span>
      </div>
      {/* 额度。两源都没有就不画条 —— 给一个「剩 100%」比不给更糟，
          那是替一个根本没读到的数字打包票。有刷新图标的那一处（总览）
          画一句「还没有额度读数」，让图标有地方待。 */}
      {(s.usage || (onRefresh && s.logged_in)) && (
        <SlotUsageBars
          usage={s.usage ?? null}
          onRefresh={onRefresh}
          refreshing={refreshing}
        />
      )}
    </div>
  );
}
