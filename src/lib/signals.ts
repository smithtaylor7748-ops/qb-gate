/**
 * 中文环境识别 —— 十项加权指纹，满分 100。
 *
 * ─────────────────────────────────────────────────────────────────────
 *  本文件的检测逻辑改编自 FuckClaude，MIT 许可：
 *    https://github.com/LinXiaoTao/FuckClaude
 *    Copyright (c) 2026 LinXiaoTao
 *  权重表、评分函数与字体/UA 名单基本保持原样，只做了本地化注释与类型微调。
 *  完整许可与致谢见仓库根目录 ATTRIBUTION.md。
 * ─────────────────────────────────────────────────────────────────────
 *
 * 两条容易被改错的判定，动手前先读：
 *   - **台湾不计分。** Asia/Taipei 与 zh-TW 是 Anthropic 完整支持的地区，
 *     给它们加分是误报。香港 / 澳门属受限地区，保留部分风险分。
 *   - **繁体字体压在命中阈值以下。** 繁体字体在台湾很常见，
 *     且区分不出 TW 与 HK/MO，分数给到 0.2（阈值 0.25）刚好不算命中。
 *
 * 全部计算在本地完成，不上传任何数据。
 */

export type SignalId =
  | "timezone"
  | "timezoneOffset"
  | "language"
  | "intlLocale"
  | "fonts"
  | "vendorFonts"
  | "cnBrowser"
  | "deviceVendor"
  | "webrtcLeak"
  | "emoji";

export interface DetectOutcome {
  /** 检测到的原始值，直接显示给用户。 */
  raw: string;
  /** 0..1，「有多像中国用户」。 */
  score: number;
}

export interface SignalDef {
  id: SignalId;
  label: string;
  weight: number;
  /** Claude Code 真正会读的信号 —— 界面上要单独标出来。 */
  claudeUsed?: boolean;
  /** 能不能修。中文字体这类修不掉的要如实说，不要假装能修。 */
  fixable: boolean;
  hint: string;
  detect: () => DetectOutcome | Promise<DetectOutcome>;
}

export const CN_TIMEZONES = [
  "Asia/Shanghai",
  "Asia/Urumqi",
  "Asia/Chongqing",
  "Asia/Chungking",
  "Asia/Harbin",
  "Asia/Kashgar",
];
export const CLAUDE_TIMEZONES = ["Asia/Shanghai", "Asia/Urumqi"];
/** 港澳属 Anthropic 受限地区，部分风险；台湾完整支持，不计分。 */
export const GREATER_CN_TIMEZONES = ["Asia/Hong_Kong", "Asia/Macau"];

const FONTS_SC = [
  "Microsoft YaHei",
  "Microsoft YaHei UI",
  "SimSun",
  "NSimSun",
  "SimHei",
  "KaiTi",
  "FangSong",
  "DengXian",
  "PingFang SC",
  "Hiragino Sans GB",
  "STHeiti",
  "STSong",
  "Songti SC",
  "Source Han Sans CN",
  "Source Han Sans SC",
  "Noto Sans CJK SC",
  "Noto Serif CJK SC",
  "WenQuanYi Micro Hei",
  "WenQuanYi Zen Hei",
];

const FONTS_TC = [
  "Microsoft JhengHei",
  "PMingLiU",
  "MingLiU",
  "DFKai-SB",
  "PingFang TC",
  "PingFang HK",
  "Source Han Sans TW",
  "Noto Sans CJK TC",
];

/** 国产厂商字体 / 国产软件带的字体（WPS 会装方正 FZ 系列）。命中一个就很说明问题。 */
const FONTS_CN_VENDOR = [
  "MiSans",
  "MIUI",
  "HarmonyOS Sans SC",
  "HarmonyOS Sans",
  "HONOR Sans",
  "OPPO Sans",
  "vivo Sans",
  "Alibaba PuHuiTi",
  "Alibaba Sans",
  "DingTalk JinBuTi",
  "Douyin Sans",
  "HYQiHei",
  "FZShuSong-Z01S",
  "FZKai-Z03S",
  "FZHei-B01S",
  "FZFangSong-Z02S",
];

const CN_BROWSER_PATTERNS: Array<[RegExp, string]> = [
  [/micromessenger|wxwork/i, "微信"],
  [/mqqbrowser|qqbrowser|\bqq\//i, "QQ 浏览器"],
  [/quark/i, "夸克"],
  [/ucbrowser|ucweb/i, "UC 浏览器"],
  [/baiduboxapp|bidubrowser|baidubrowser/i, "百度"],
  [/miuibrowser|xiaomi\/|mibrowser/i, "小米浏览器"],
  [/huaweibrowser/i, "华为浏览器"],
  [/heytapbrowser|oppobrowser/i, "OPPO 浏览器"],
  [/vivobrowser/i, "vivo 浏览器"],
  [/sogoumobilebrowser|\bmetasr\b|\bse 2\.x\b/i, "搜狗"],
  [/maxthon/i, "傲游"],
  [/360se|360ee|qihoobrowser|\bqhbrowser\b/i, "360 浏览器"],
  [/2345explorer|2345browser/i, "2345"],
  [/lbbrowser/i, "猎豹"],
  [/theworld/i, "世界之窗"],
  [/aweme|bytedancewebview|newsarticle|toutiaomicroapp/i, "抖音 / 今日头条"],
  [/alipayclient/i, "支付宝"],
  [/dingtalk/i, "钉钉"],
  [/weibo/i, "微博"],
  [/xiaohongshu|xhsminiapp/i, "小红书"],
  [/\bbilibili\b/i, "B 站"],
];

/** 国产设备 / 系统。鸿蒙是决定性的；全球也卖的品牌分数低些。 */
const CN_DEVICE_PATTERNS: Array<[RegExp, string, number]> = [
  [/harmonyos|openharmony/i, "HarmonyOS", 1],
  [/huawei|\bhonor\b/i, "华为 / 荣耀", 0.8],
  [/meizu/i, "魅族", 0.8],
  [/nubia|\bzte\b/i, "中兴 / 努比亚", 0.7],
  [/xiaomi|redmi|\bpoco\b|\bm2\d{3}[a-z0-9]+\b/i, "小米", 0.6],
  [/oppo|\bpd[a-z]m\d{2}\b/i, "OPPO", 0.6],
  [/vivo|\bv2\d{3}[a-z]{1,2}\b/i, "vivo", 0.6],
  [/realme|\brmx\d{4}\b/i, "realme", 0.6],
  [/oneplus/i, "一加", 0.6],
  [/\blenovo\b|\bzuk\b/i, "联想", 0.5],
];

function getTimezone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "";
  } catch {
    return "";
  }
}

export function scoreTimezone(tz: string): number {
  if (CLAUDE_TIMEZONES.includes(tz) || CN_TIMEZONES.includes(tz)) return 1;
  if (GREATER_CN_TIMEZONES.includes(tz)) return 0.6;
  return 0;
}

function detectTimezone(): DetectOutcome {
  const tz = getTimezone();
  return { raw: tz || "未知", score: scoreTimezone(tz) };
}

function detectTimezoneOffset(): DetectOutcome {
  const offset = new Date().getTimezoneOffset();
  const utcHours = -offset / 60;
  const sign = utcHours >= 0 ? "+" : "-";
  return {
    raw: `UTC${sign}${Math.abs(utcHours)}`,
    score: offset === -480 ? 0.7 : 0,
  };
}

function normLangs(): string[] {
  const list =
    navigator.languages && navigator.languages.length
      ? navigator.languages
      : [navigator.language];
  return list.map((l) => (l || "").toLowerCase());
}

/**
 * zh-TW 是台湾（完整支持），不计分 —— 包括浏览器跟在它后面补的裸 "zh"。
 * zh-HK / zh-MO 保留部分风险。
 */
export function scoreLanguages(langs: string[]): number {
  const list = langs.map((l) => (l || "").toLowerCase()).filter(Boolean);
  const isTW = (l: string) =>
    l.startsWith("zh-tw") || (l.includes("hant") && l.includes("tw"));
  const isHKMO = (l: string) => l.startsWith("zh-hk") || l.startsWith("zh-mo");
  const isTrad = (l: string) => isTW(l) || isHKMO(l) || l.includes("hant");
  const firstTrad = list.findIndex(isTrad);

  const isHansCN = (l: string, i: number) =>
    l.startsWith("zh-cn") ||
    l.includes("hans") ||
    (l === "zh" && (firstTrad === -1 || i < firstTrad));

  const kept = list
    .map((l, i) => ({ l, i }))
    .filter(
      ({ l, i }) =>
        !isTW(l) && !(l === "zh" && firstTrad !== -1 && i > firstTrad),
    );

  const primary = kept[0];
  if (!primary) return 0;
  if (isHansCN(primary.l, primary.i)) return 1;
  if (isHKMO(primary.l) || primary.l.includes("hant")) return 0.5;
  if (kept.some(({ l, i }) => isHansCN(l, i))) return 0.7;
  if (kept.some(({ l }) => l.startsWith("zh"))) return 0.4;
  return 0;
}

/**
 * 把语言变体的判定说出来。
 *
 * `scoreLanguages` 早就把 zh-CN / zh-HK·MO / zh-TW 分成三档了，但界面上只看得到
 * 一串语言标签和一根进度条 —— 使用者不知道 zh-TW 是**被有意不计分**的，
 * 只会觉得「我明明是中文却没扣分，这检测是不是坏了」。
 *
 * check-cc 把变体拆成一项独立信号（权重 12）。这里不拆：拆了要重摊权重，
 * 而判定逻辑本来就在，缺的只是把结论写出来。
 */
function languageVerdict(langs: string[]): string {
  const l = langs.map((x) => (x || "").toLowerCase());
  if (
    l.some(
      (x) => x.startsWith("zh-tw") || (x.includes("hant") && x.includes("tw")),
    )
  ) {
    return "台湾（Anthropic 完整支持地区，不计分）";
  }
  if (l.some((x) => x.startsWith("zh-hk") || x.startsWith("zh-mo"))) {
    return "港澳（受限地区，计部分风险分）";
  }
  if (
    l.some((x) => x.startsWith("zh-cn") || x.includes("hans") || x === "zh")
  ) {
    return "简体中文（计满分风险）";
  }
  return "未检出中文";
}

function detectLanguage(): DetectOutcome {
  const langs = normLangs();
  const raw = langs.join(", ") || "未知";
  return {
    raw: `${raw} · ${languageVerdict(langs)}`,
    score: scoreLanguages(langs),
  };
}

function detectIntlLocale(): DetectOutcome {
  let locale = "";
  try {
    locale = Intl.DateTimeFormat().resolvedOptions().locale || "";
  } catch {
    locale = "";
  }
  const l = locale.toLowerCase();
  let score = 0;
  if (l.startsWith("zh-cn") || l.includes("hans") || l === "zh") score = 1;
  else if (l.startsWith("zh") && !l.startsWith("zh-tw")) score = 0.5;
  return { raw: locale || "未知", score };
}

/** canvas 宽度探测：装了这个字体，同一串字的渲染宽度会和回退字体不同。 */
function isFontAvailable(font: string, ctx: CanvasRenderingContext2D): boolean {
  const testString = "中文字体检测ABCabc012";
  const size = "72px";
  const bases = ["monospace", "sans-serif", "serif"];
  return bases.some((base) => {
    ctx.font = `${size} ${base}`;
    const baseWidth = ctx.measureText(testString).width;
    ctx.font = `${size} "${font}", ${base}`;
    const testWidth = ctx.measureText(testString).width;
    return Math.abs(testWidth - baseWidth) > 0.5;
  });
}

function fontCtx(): CanvasRenderingContext2D | null {
  return document.createElement("canvas").getContext("2d");
}

function detectFonts(): DetectOutcome {
  const ctx = fontCtx();
  if (!ctx) return { raw: "canvas 不可用", score: 0 };

  const sc = FONTS_SC.filter((f) => isFontAvailable(f, ctx));
  const tc = FONTS_TC.filter((f) => isFontAvailable(f, ctx));

  let score = 0;
  if (sc.length >= 1) score = Math.min(1, 0.75 + 0.08 * sc.length);
  else if (tc.length >= 1) score = 0.2;

  const hit = [...sc, ...tc];
  const raw = hit.length
    ? hit.slice(0, 4).join(", ") + (hit.length > 4 ? "…" : "")
    : "未检测到";
  return { raw, score };
}

function detectVendorFonts(): DetectOutcome {
  const ctx = fontCtx();
  if (!ctx) return { raw: "canvas 不可用", score: 0 };
  const hit = FONTS_CN_VENDOR.filter((f) => isFontAvailable(f, ctx));
  const score = hit.length >= 2 ? 1 : hit.length === 1 ? 0.8 : 0;
  const raw = hit.length
    ? hit.slice(0, 3).join(", ") + (hit.length > 3 ? "…" : "")
    : "未检测到";
  return { raw, score };
}

type UADataBrand = { brand: string; version: string };
interface UAData {
  brands?: UADataBrand[];
  getHighEntropyValues?: (hints: string[]) => Promise<Record<string, unknown>>;
}

function uaData(): UAData | undefined {
  return (navigator as Navigator & { userAgentData?: UAData }).userAgentData;
}

export function scoreCnBrowser(probe: string): {
  name: string | null;
  score: number;
} {
  for (const [re, name] of CN_BROWSER_PATTERNS) {
    if (re.test(probe)) return { name, score: 1 };
  }
  return { name: null, score: 0 };
}

function detectCnBrowser(): DetectOutcome {
  const brands = (uaData()?.brands ?? []).map((b) => b.brand).join(" ");
  const { name, score } = scoreCnBrowser(`${navigator.userAgent} ${brands}`);
  return { raw: name ?? "未检测到", score };
}

export function scoreCnDevice(probe: string): {
  name: string | null;
  score: number;
} {
  for (const [re, name, score] of CN_DEVICE_PATTERNS) {
    if (re.test(probe)) return { name, score };
  }
  return { name: null, score: 0 };
}

async function detectDeviceVendor(): Promise<DetectOutcome> {
  let extra = "";
  try {
    const high = await uaData()?.getHighEntropyValues?.([
      "model",
      "platform",
      "platformVersion",
    ]);
    if (high)
      extra = ` ${String(high.model ?? "")} ${String(high.platform ?? "")}`;
  } catch {
    /* 客户端拒绝了高熵提示，退回裸 UA */
  }
  const { name, score } = scoreCnDevice(`${navigator.userAgent}${extra}`);
  return { raw: name ?? "未检测到", score };
}

export function scoreEmojiVendor(probe: string): {
  vendor: string;
  score: number;
} {
  const p = probe.toLowerCase();
  let vendor = "未知";
  if (/iphone|ipad|ipod|mac/.test(p)) vendor = "Apple";
  else if (/android/.test(p)) vendor = "Google";
  else if (/win/.test(p)) vendor = "Microsoft";
  else if (/cros/.test(p)) vendor = "Google";
  else if (/linux/.test(p)) vendor = "Linux / 其他";

  const vendorScore: Record<string, number> = {
    Apple: 0.25,
    Microsoft: 0.4,
    Google: 0.35,
    "Linux / 其他": 0.5,
    未知: 0.4,
  };
  return { vendor, score: vendorScore[vendor] ?? 0.4 };
}

function detectEmoji(): DetectOutcome {
  const ua = (navigator.userAgent || "").toLowerCase();
  const platform = (navigator.platform || "").toLowerCase();
  const { vendor, score } = scoreEmojiVendor(`${platform} ${ua}`);
  return { raw: `${vendor} 风格`, score };
}

/**
 * WebRTC 收到的 ICE 候选地址。手法与 DNSLeakTester / webrtc-privacy 同源：STUN + 收候选。
 *
 * 单独导出（2026-09-24）：真实浏览器采集那一页要把地址本身交回面板，
 * 面板拿它跟出口 IP 比 —— 候选地址就是出口的话不算泄露，是别的地址才算。
 */
export function webrtcCandidates(): Promise<string[]> {
  return new Promise((resolve) => {
    if (typeof window === "undefined" || !window.RTCPeerConnection) {
      resolve([]);
      return;
    }

    let resolved = false;
    const ips: string[] = [];
    let pc: RTCPeerConnection | null = null;

    const finish = () => {
      if (resolved) return;
      resolved = true;
      try {
        pc?.close();
      } catch {
        /* 已经关了 */
      }
      resolve(ips);
    };

    try {
      pc = new RTCPeerConnection({
        iceServers: [{ urls: "stun:stun.l.google.com:19302" }],
      });
      pc.onicecandidate = (e) => {
        if (!e.candidate?.candidate) {
          finish();
          return;
        }
        const m =
          /([0-9]{1,3}(\.[0-9]{1,3}){3}|[a-f0-9]{1,4}(:[a-f0-9]{1,4}){7})/i.exec(
            e.candidate.candidate,
          );
        if (m?.[1] && !ips.includes(m[1])) ips.push(m[1]);
      };
      pc.createDataChannel("");
      pc.createOffer()
        .then((sdp) => pc?.setLocalDescription(sdp))
        .catch(() => finish());
    } catch {
      finish();
    }

    setTimeout(finish, 1000);
  });
}

/** WebRTC ICE 候选泄露。 */
async function detectWebrtcLeak(): Promise<DetectOutcome> {
  const ips = await webrtcCandidates();
  if (ips.length === 0) return { raw: "未发现泄露", score: 0 };
  return {
    raw: `候选地址泄露（${ips.slice(0, 2).join(", ")}）`,
    score: 0.5,
  };
}

export const SIGNALS: SignalDef[] = [
  {
    id: "timezone",
    label: "系统时区",
    weight: 24,
    claudeUsed: true,
    fixable: true,
    hint: "Intl 读到的就是 Claude Code 读取的同一个系统时区。这是权重最高、也最好修的一项。",
    detect: detectTimezone,
  },
  {
    id: "language",
    label: "浏览器语言",
    weight: 18,
    fixable: true,
    hint: "看 navigator.languages。把 zh-CN / zh-Hans 从列表里去掉即可。",
    detect: detectLanguage,
  },
  {
    id: "fonts",
    label: "已装中文字体",
    weight: 14,
    fixable: false,
    hint: "canvas 宽度探测。删掉系统中文字体会让中文显示整体崩坏，代价远大于收益，本面板不提供「修复」。",
    detect: detectFonts,
  },
  {
    id: "vendorFonts",
    label: "国产厂商字体",
    weight: 10,
    fixable: true,
    hint: "MiSans、鸿蒙、方正等。多为国产软件（如 WPS）安装，卸载对应软件即可消除。",
    detect: detectVendorFonts,
  },
  {
    id: "webrtcLeak",
    label: "WebRTC 泄露",
    weight: 10,
    fixable: true,
    hint: "收 ICE 候选看有没有暴露真实地址。浏览器策略或扩展可以关掉。",
    detect: detectWebrtcLeak,
  },
  {
    id: "cnBrowser",
    label: "国产浏览器",
    weight: 8,
    fixable: true,
    hint: "UA 与 UA-CH 品牌匹配。换用官方 Chrome / Edge 即可。",
    detect: detectCnBrowser,
  },
  {
    id: "deviceVendor",
    label: "国产设备",
    weight: 6,
    fixable: false,
    hint: "设备品牌来自 UA，桌面端通常不命中。",
    detect: detectDeviceVendor,
  },
  {
    id: "intlLocale",
    label: "Intl 区域设置",
    weight: 4,
    fixable: true,
    hint: "日期数字格式化用的 locale，跟随系统区域格式设置。",
    detect: detectIntlLocale,
  },
  {
    id: "timezoneOffset",
    label: "时区偏移",
    weight: 3,
    fixable: true,
    hint: "getTimezoneOffset() 是否为 UTC+8。改系统时区后自动跟着变。",
    detect: detectTimezoneOffset,
  },
  {
    id: "emoji",
    label: "Emoji 渲染风格",
    weight: 3,
    fixable: false,
    hint: "由 UA 推断操作系统厂商，弱相关信号，基本无法也无必要修改。",
    detect: detectEmoji,
  },
];

export type RiskBand = "low" | "medium" | "high";

/** 分档：低 0–30、中 31–60、高 61–100。 */
export function riskBand(total: number): RiskBand {
  if (total <= 30) return "low";
  if (total <= 60) return "medium";
  return "high";
}

/** 单项判定。score ≥ 0.25 算命中。 */
export function signalVerdict(score: number): RiskBand {
  if (score >= 0.6) return "high";
  if (score >= 0.25) return "medium";
  return "low";
}

export interface SignalResult extends SignalDef {
  raw: string;
  score: number;
  /** 该项实际贡献的分数（四舍五入到整数）。 */
  points: number;
  verdict: RiskBand;
}

export interface ScanResult {
  total: number;
  band: RiskBand;
  signals: SignalResult[];
  hits: SignalResult[];
  /**
   * 在哪量的（2026-09-24）。没有 = 面板内置的 WebView2（老数据也是这一档）；
   * `browser` = 用默认浏览器实测的（`lib/browserProbe.ts`）。
   */
  source?:
    | { kind: "panel" }
    | { kind: "browser"; browser: string; at: string };
}

/**
 * 一项的原始采集结果。可以序列化 —— 真实浏览器采集那一页（`src/probe/main.ts`）
 * 在用户的浏览器里跑同一份 `detect()`，把这个交回面板。
 */
export interface RawSignal {
  id: SignalId;
  raw: string;
  score: number;
}

/** 在**当前这个浏览器**里跑一遍十项采集。 */
export async function collectSignals(): Promise<RawSignal[]> {
  const out: RawSignal[] = [];
  for (const def of SIGNALS) {
    let outcome: DetectOutcome;
    try {
      outcome = await def.detect();
    } catch {
      outcome = { raw: "检测失败", score: 0 };
    }
    out.push({ id: def.id, raw: outcome.raw, score: outcome.score });
  }
  return out;
}

/**
 * 把原始结果按权重表算成总分。**分数只在这里算** —— 面板自己采的、真实浏览器交回来的，
 * 走同一个函数。交回来的东西当不可信数据：认不出的项丢掉，分数夹在 0–1。
 */
export function scanFrom(raw: RawSignal[]): ScanResult {
  const byId = new Map(raw.map((r) => [r.id, r]));
  const signals: SignalResult[] = SIGNALS.map((def) => {
    const r = byId.get(def.id);
    const score = Math.min(1, Math.max(0, Number(r?.score) || 0));
    return {
      ...def,
      raw: typeof r?.raw === "string" ? r.raw.slice(0, 200) : "没有采到",
      score,
      points: Math.round(score * def.weight),
      verdict: signalVerdict(score),
    };
  });
  const total = Math.round(signals.reduce((s, x) => s + x.score * x.weight, 0));
  return {
    total,
    band: riskBand(total),
    signals,
    hits: signals.filter((s) => s.score >= 0.25),
  };
}

export async function runScan(): Promise<ScanResult> {
  return { ...scanFrom(await collectSignals()), source: { kind: "panel" } };
}
