/**
 * 槽位横条上的额度：五小时 / 七天两条 + 恢复时刻 + 来源与时刻。
 *
 * 数据全部来自官方客户端写在本机的文件（见 `src-tauri/src/accounts/usage.rs`），
 * 零网络请求、零接口调用。
 *
 * # 条子为什么是「两段都上色」，不是一根进度条 ★
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
 * # 两个窗口里紧的那个才是你真正能用的 ★
 *
 * 实机上出现过 5 小时剩 88%、7 天剩 21%。只看前一个会以为很宽裕，
 * 实际上七天窗口一到顶，五小时再空也发不出请求。所以紧的那个标「卡这儿」，
 * 并且整行的语气色取紧的那个 —— 面板不能让人看着宽裕的那半做判断。
 *
 * # 三件必须如实说的事
 *
 * 1. **来源和时刻要标出来。** 两个槽位的来源经常不一样 —— 桌面端当前登录的
 *    那个有分钟级样本，别的槽位只有 Claude Code 留下的快照。
 * 2. **读数跨过窗口就不是当前值。** 五小时的读数放了六小时，它记的用量
 *    必然已经作废。这种情况显示「读数已过期」而不是那个数字 ——
 *    显示旧数字会让人以为额度还剩那么多。
 *
 *    ⛔ **这条对两个来源一视同仁。** 0.19.2 之前只判 `source === "cache"`，
 *    桌面端那一档再旧也理直气壮地当当前值显示。可是桌面端的样本
 *    只在它运行时才写（`usage.rs` 文件头），关掉一天读数就冻在最后一条，
 *    而那一天里窗口早就重置过了。过期就是过期，跟谁写的没关系；
 *    来源仍然标出来，那决定的是使用者该去开桌面端还是去跑一次 Claude Code。
 * 3. **推算的恢复时刻要标「推算」。** 桌面端那份只有用量没有重置时刻，
 *    拿不到实测值时是从样本的断崖下跌反推的，误差约半个采样间隔。
 */
import type { SlotUsage, UsageWindow } from "../../lib/api";

/** 五小时窗口的分钟数。读数比这还旧，那份五小时读数必然跨过了窗口。 */
const FIVE_HOUR_MIN = 5 * 60;
const SEVEN_DAY_MIN = 7 * 24 * 60;

const SOURCE_NAME: Record<SlotUsage["source"], string> = {
  desktop: "桌面端",
  cache: "Claude Code 快照",
};

/** 恢复时刻：今天之内只给时分，跨天补上日期。 */
function fmtReset(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const hhmm = `${String(d.getHours()).padStart(2, "0")}:${String(
    d.getMinutes(),
  ).padStart(2, "0")}`;
  const sameDay = d.toDateString() === new Date().toDateString();
  return sameDay
    ? hhmm
    : `${d.getMonth() + 1}-${String(d.getDate()).padStart(2, "0")} ${hhmm}`;
}

function fmtAge(min: number): string {
  if (min < 1) return "刚刚";
  if (min < 60) return `${min} 分钟前`;
  if (min < 60 * 24) return `${Math.round(min / 60)} 小时前`;
  return `${Math.round(min / (60 * 24))} 天前`;
}

function tone(left: number): "ok" | "warn" | "danger" {
  if (left >= 50) return "ok";
  if (left >= 20) return "warn";
  return "danger";
}

/**
 * 这份读数是不是已经跨过了那个窗口。
 *
 * 不分来源 —— 理由见文件头第 2 条。
 */
function expiredIn(usage: SlotUsage, limit: number): boolean {
  return usage.age_minutes > limit;
}

function Gauge({
  name,
  window: w,
  expired,
  binding,
}: {
  name: string;
  window: UsageWindow | null | undefined;
  expired: boolean;
  /** 两个窗口里紧的那个。整行的判断要以它为准。 */
  binding: boolean;
}) {
  // 没这一项就整条不画。画一条空槽会被读成「剩 0」。
  if (!w) return null;

  if (expired) {
    return (
      <span className="gauge gauge--dead">
        <span className="gauge-name">{name}</span>
        <span className="gauge-note">读数已过期，不是当前值</span>
      </span>
    );
  }

  const left = Math.max(0, 100 - w.used);
  return (
    <span className="gauge">
      <span className="gauge-name">{name}</span>
      <span
        className="gauge-track"
        title={`已用 ${w.used}%，剩 ${left}%`}
        role="img"
        aria-label={`${name}已用 ${w.used}%，剩 ${left}%`}
      >
        {/* 上色的那段 = 已经用掉的。空槽 = 还剩的。
            颜色按剩余量给：长度说「用了多少」，颜色说「还够不够」。 */}
        <span
          className={`gauge-used gauge-used--${tone(left)}`}
          style={{ width: `${w.used}%` }}
        />
      </span>
      <strong className={`gauge-pct qb-tone-${tone(left)}`}>剩 {left}%</strong>
      {binding && (
        <span
          className="gauge-binding"
          title="两个窗口里紧的那个，先到顶的就是它"
        >
          卡这儿
        </span>
      )}
      {w.resets_at ? (
        <span
          className="gauge-reset"
          title={
            w.estimated
              ? "从样本里那次断崖下跌反推的，不是官方客户端写下的时刻，误差约七八分钟"
              : "官方客户端写下的重置时刻"
          }
        >
          ↻ {fmtReset(w.resets_at)}
          {w.estimated && <em>推算</em>}
        </span>
      ) : (
        <span className="gauge-reset" title="两源都没给出这个窗口的重置时刻">
          ↻ 未知
        </span>
      )}
    </span>
  );
}

export default function SlotUsageBars({ usage }: { usage: SlotUsage }) {
  const fhDead = expiredIn(usage, FIVE_HOUR_MIN);
  const sdDead = expiredIn(usage, SEVEN_DAY_MIN);

  // 紧的那个：两边都有有效读数时才比得出来，差距太小就不标
  // （都在 50% 上下时标一个「卡这儿」只是噪音）。
  const fh = fhDead ? null : usage.five_hour;
  const sd = sdDead ? null : usage.seven_day;
  let bind: "five" | "seven" | null = null;
  if (fh && sd && Math.abs(fh.used - sd.used) >= 10) {
    bind = fh.used > sd.used ? "five" : "seven";
  }

  return (
    <div className="slotusage">
      <Gauge
        name="5 小时"
        window={usage.five_hour}
        expired={fhDead}
        binding={bind === "five"}
      />
      <Gauge
        name="7 天"
        window={usage.seven_day}
        expired={sdDead}
        binding={bind === "seven"}
      />
      <span
        className="gauge-src"
        title={
          usage.source === "desktop"
            ? "Claude 桌面端每约 15 分钟往本机写一条样本。它没开的时候不写 —— 那期间的读数就停在最后一条。"
            : "Claude Code 会话里留下的快照。这个槽位的桌面端历史里没有更新的样本 —— 它没在桌面端登录过，或者那阵子桌面端没开。"
        }
      >
        {SOURCE_NAME[usage.source]} · {fmtAge(usage.age_minutes)}
      </span>
    </div>
  );
}
