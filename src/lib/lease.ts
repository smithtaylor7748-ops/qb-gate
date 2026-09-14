/**
 * 租约持有者怎么显示。
 *
 * `lease.holder` 是后端把 `holders` 的键用「、」拼起来的**整串**，
 * 而键里绝大多数是会话 id —— `config_io::id()` 产出的
 * `20260913-045632-ef2e6a0958ef48deb76f0b4d12ca77b1`，48 个字符，
 * 中间没有一个可断行的位置。五个会话再加一个 `claude-desktop`，
 * 拼出来三百多字符。
 *
 * 它原样落在四个地方：总览评分卡的门禁读数、IP 白名单小窗的「执行锁」
 * 那一格、安全对象摘要、评分明细。其中那一格只有四分之一屏宽 ——
 * 实机上字直接从卡片里横着溢出去，这就是这次的原始报告。
 *
 * 会话 id 里对使用者有意义的只有前面那段时间戳；后面 32 位 uuid
 * 只是用来跟同一秒启动的另一个会话区分开，显示出来没有任何信息量。
 *
 * ⚠ 四处共用这一个函数，别在别处再拼一遍。少改一处，那一处就会在
 * 同样的实机状态下重新溢出 —— 而这种 bug 只有开着五个会话的人碰得到，
 * 自己测很难复现。
 */

/** 会话 id：`YYYYMMDD-HHMMSS-` 加 32 位无连字符 uuid。 */
const SESSION_ID = /^(\d{8}-\d{6})-[0-9a-f]{32}$/i;

export interface LeaseLike {
  holder?: string | null;
  /** 后端 `BTreeMap<String, Option<WatchMode>>`，键才是持有者名字。 */
  holders?: Record<string, unknown> | null;
}

/** 单个持有者的短名。会话 id 只留时间戳那一段，其余原样。 */
export function shortHolder(name: string): string {
  const m = SESSION_ID.exec(name);
  return m ? m[1] : name;
}

/**
 * 持有者清单。
 *
 * 优先用 `holders` 的键 —— 那是真正的列表。`holder` 是拼好的整串，
 * 只在读到旧版本写下的 `lease.json` 时才需要，那时按「、」拆回来。
 */
export function holderNames(lease: LeaseLike): string[] {
  const keys = lease.holders ? Object.keys(lease.holders) : [];
  if (keys.length > 0) return keys;
  const raw = lease.holder?.trim();
  return raw ? raw.split("、").filter(Boolean) : [];
}

export interface LeaseText {
  /** 一行能放下的说法。 */
  text: string;
  /** 完整清单，一行一个，挂在 title 上给想看的人。 */
  title: string;
}

/**
 * 「已租给 …」这句话。没有持有者时返回 null，由调用方决定说什么。
 *
 * @param verb 动词部分，各处措辞不同（「已租给」/「已放行给」）。
 */
export function describeLease(lease: LeaseLike, verb: string): LeaseText | null {
  const names = holderNames(lease);
  if (names.length === 0) return null;
  const short = names.map(shortHolder);
  const text =
    names.length <= 2
      ? `${verb} ${short.join("、")}`
      : `${verb} ${short[0]} 等 ${names.length} 个`;
  return { text, title: names.join("\n") };
}
