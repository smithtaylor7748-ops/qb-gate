/**
 * `ip-gate.log` 的行解析。
 *
 * 后端写日志的格式是 `gate/log.rs` 里那一句：
 * `writeln!(f, "{stamp} {line}")`，`stamp` 走 `%m-%d %H:%M:%S`。
 * 正文是中文短句，由三十来处 `log::write` 各自拼出来 —— **没有结构化字段**，
 * 所以这里只能按措辞归类。
 *
 * # 两条设计上的取舍
 *
 * 1. **分类与语气是两回事。** 「拒绝放行：出口 IP … 不在白名单内」既属于门禁
 *    （topic = gate），又是一条错误（tone = danger）。早先的写法把出错单列成
 *    一个分类，结果是按「门禁」筛选时**恰恰漏掉最该看的那几行**。
 *    现在 `category` 只回答「这条讲的是什么」，`tone` 只回答「严不严重」，
 *    界面上的「出错」筛选看 `tone`，不看 `category`。
 *
 * 2. **认不出来的行原样留着。** 归类失败就落进 `process` + `default`，
 *    正文一个字不动。日志的第一用途是排查，删行比错分类糟得多。
 */

/** 这条日志讲的是什么。**不含「出错」** —— 出错是语气，见上面第 1 条。 */
export type LogCategory = 'gate' | 'account' | 'process';

export type LogTone = 'default' | 'ok' | 'warn' | 'danger';

/** 行内快捷动作。目前只有一种。 */
export interface LogAction {
  kind: 'allowlist';
  ip: string;
}

export interface LogEntry {
  /** 原始整行。复制全部时用它，保证跟文件里一模一样。 */
  raw: string;
  /** `MM-DD HH:MM:SS`。认不出来是 `null`，这时 `text` 就是整行。 */
  stamp: string | null;
  /** 去掉时间戳之后的正文。 */
  text: string;
  category: LogCategory;
  tone: LogTone;
  action?: LogAction;
}

/** `09-09 10:55:02 已上锁 5 个可执行文件` */
const STAMP = /^(\d{2}-\d{2} \d{2}:\d{2}:\d{2})\s+(.*)$/;

/**
 * 「拒绝放行：出口 IP {ip} 不在白名单内」——`gate/mod.rs` 里写死的那句。
 *
 * 按整句形状抓而不是抓 IPv4 字面量，这样 IPv6 也能抓到，
 * 也不会误伤正文里碰巧出现的别的地址。
 */
const DENIED = /出口 IP (\S+) 不在白名单内/;

const DANGER = ['拒绝', '失败', '错误', '查不到'];
// 「锁不上」：`已上锁 N 个，另有 M 个锁不上` —— 部分失败，门没全关上（v0.8.0）。
const WARN = ['收摊', '应急解锁', '白名单为空', '可绕过', '锁不上'];
const OK = ['已放行', '已就绪', '已完成'];

const ACCOUNT = ['账户', '中转站', '槽位'];
// 「重锁」要单列：`面板退出时重锁失败` 里没有「上锁」二字，
// 少了它这条门禁事件会掉进 process。
const GATE = ['上锁', '重锁', '解锁', '放行', '租约', '白名单', '残留副本', '门禁', '执行锁'];

function hit(text: string, words: string[]): boolean {
  return words.some((w) => text.includes(w));
}

function toneOf(text: string): LogTone {
  if (hit(text, DANGER)) return 'danger';
  if (hit(text, WARN)) return 'warn';
  if (hit(text, OK)) return 'ok';
  return 'default';
}

function categoryOf(text: string): LogCategory {
  // v0.8.0 起启动那一行会带上用的是哪个账户槽位
  // （`Claude Code 已放行并启动（…，账户槽位 main）`）。它首先是一次门禁放行 ——
  // 按「账户」归类的话，按「门禁」筛选时就看不到放行记录了。
  if (text.includes('放行')) return 'gate';
  if (hit(text, ACCOUNT)) return 'account';
  if (hit(text, GATE)) return 'gate';
  return 'process';
}

export function parseLogLine(raw: string): LogEntry {
  const m = STAMP.exec(raw);
  const stamp = m ? m[1] : null;
  const text = m ? m[2] : raw;

  const entry: LogEntry = {
    raw,
    stamp,
    text,
    category: categoryOf(text),
    tone: toneOf(text),
  };

  const denied = DENIED.exec(text);
  if (denied) entry.action = { kind: 'allowlist', ip: denied[1] };

  return entry;
}

export function parseLog(lines: string[]): LogEntry[] {
  return lines.map(parseLogLine);
}

/** 界面上的四个筛选片。`all` 与 `error` 不是分类，所以单独一个类型。 */
export type LogFilter = 'all' | LogCategory | 'error';

export const LOG_FILTER_LABEL: Record<LogFilter, string> = {
  all: '全部',
  gate: '门禁',
  account: '账户',
  process: '启停',
  error: '出错',
};

/** 「出错」看 `tone`，其余看 `category` —— 见文件头第 1 条。 */
export function matchesFilter(e: LogEntry, f: LogFilter): boolean {
  if (f === 'all') return true;
  if (f === 'error') return e.tone === 'danger';
  return e.category === f;
}
