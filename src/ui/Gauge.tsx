/**
 * 一根额度条。Claude / GPT / 反重力 三个账户页共用（0.32.0 从 `SlotUsage.tsx` 抽出来）。
 *
 * # 条子为什么是「上色的那段就是用掉的」★
 *
 * 这一段是从 `SlotUsage.tsx` 原样搬过来的，**别改**：它记着三个被否掉的设计。
 *
 * 第一版填的是**剩余**：剩 21% 就画 21% 宽。使用者当场读反了 ——
 * 「我看着还以为有很多额度」。这不怪他：进度条的通用语义是「已经进行了多少」，
 * 一根细细的条读出来就是「才用了一点点」，而真相是只剩一点点。
 *
 * 换成填「已用」也不解决问题，因为旁边的数字写的是「剩」，
 * 条和字各说各的，还是要在脑子里换算一次。
 *
 * 第二版两段都上色（用掉的浅灰、剩下的有色），还是不对：七天只剩 19% 时，
 * 右边那截红很短，**左边那段长长的浅色反而像「可用的」**。
 *
 * 现在是第三版，也是最朴素的那种：**上色的那段就是用掉的**，剩下的留空。
 * 跟磁盘占用条一个读法 —— 条子越长用得越多，而且颜色按**剩余量**变
 * （剩得多是绿、剩得少是红），长度和颜色指向同一个结论，没有反读的余地。
 *
 * ⛔ **所以传进来的永远是 `used`（已用 0–100），不是 `remaining`。**
 * 反重力那边的数据是 0–1 的剩余比例，换算在调用处做一次
 * （`100 - remaining * 100`）—— 两页的条子必须是同一个含义，
 * 否则同一根绿条在两个页面上说的是相反的事。
 *
 * # 「没测到」不是 0 ★
 *
 * `used == null` 画的是斜纹空槽（`gauge-used--none`），跟「测过、是 0%」分得开。
 * 反重力的 `remaining === null` 就是这一档（`lib/antigravityQuota.ts` 钉着这条规矩），
 * Codex 那边「还没有带额度信息的会话记录」也是。
 * 画成实心 0 就是替使用者断言了一件没人知道的事。
 */
import type { ReactNode } from "react";

export type GaugeTone = "ok" | "warn" | "danger";

/** 剩得多是绿、剩得少是红。判定按**剩余**，跟条长方向相反，这是故意的（见文件头）。 */
export function gaugeTone(left: number): GaugeTone {
  if (left >= 50) return "ok";
  if (left >= 20) return "warn";
  return "danger";
}

export interface GaugeProps {
  /** 左边那列的名字，例如「5 小时」「7 天」「Gemini 3.8 Pro」。 */
  name: ReactNode;
  /**
   * **已用**百分比 0–100。`null` = 没测到 —— 画斜纹，不画 0。
   */
  used: number | null;
  /** 右边那列。不给就按 `剩 N%` 自动生成。 */
  value?: ReactNode;
  /** 名字下面/后面的补充（重置时刻之类）。 */
  extra?: ReactNode;
  /** 这一根是「卡住你的那个」。整行的语气色应当以它为准。 */
  binding?: boolean;
  /** 整根换成一句话（读数过期之类）。给了它就不画条。 */
  note?: ReactNode;
  className?: string;
}

export default function Gauge({
  name,
  used,
  value,
  extra,
  binding,
  note,
  className = "",
}: GaugeProps) {
  if (note) {
    return (
      <span className={`gauge gauge--dead ${className}`}>
        <span className="gauge-name">{name}</span>
        <span className="gauge-note">{note}</span>
      </span>
    );
  }

  // 没测到：斜纹空槽 + 「—」。**不画 0。**
  if (used == null || !Number.isFinite(used)) {
    return (
      <span className={`gauge ${className}`}>
        <span className="gauge-name">{name}</span>
        <span
          className="gauge-track"
          title="这一项没有读到额度信息 —— 不是 0"
          role="img"
          aria-label={`${typeof name === "string" ? name : ""}没有读到额度信息`}
        >
          <span
            className="gauge-used gauge-used--none"
            style={{ width: "100%" }}
          />
        </span>
        <strong className="gauge-pct qb-st-dim">—</strong>
        {extra}
      </span>
    );
  }

  const u = Math.max(0, Math.min(100, used));
  const left = Math.max(0, 100 - Math.round(u));
  const t = gaugeTone(left);
  const label = `${typeof name === "string" ? name : ""}已用 ${Math.round(u)}%，剩 ${left}%`;
  return (
    <span className={`gauge ${className}`}>
      <span className="gauge-name">{name}</span>
      <span className="gauge-track" title={label} role="img" aria-label={label}>
        {/* 上色的那段 = 已经用掉的。空槽 = 还剩的。
            颜色按剩余量给：长度说「用了多少」，颜色说「还够不够」。 */}
        <span
          className={`gauge-used gauge-used--${t}`}
          style={{ width: `${u}%` }}
        />
      </span>
      <strong className={`gauge-pct qb-tone-${t}`}>
        {value ?? `剩 ${left}%`}
      </strong>
      {binding && (
        <span
          className="gauge-binding"
          title="两个窗口里紧的那个，先到顶的就是它"
        >
          卡这儿
        </span>
      )}
      {extra}
    </span>
  );
}
