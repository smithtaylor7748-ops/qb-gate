/** QB 独立编写的检测任务；分项证据不合并为模型身份或“降智”判决。 */
export const QUALITY_CHECK_VERSION = "qb-quality-v2";
export const CANDY_VERSION = "qb-candy-shape-v1";
export const ANIMATION_VERSION = "qb-lighthouse-css-v1";
export const QUALITY_HISTORY_KEY = "qb-quality-check-v2";

export const candyPrompt = `请独立解答这个保证问题，不调用工具。
袋中糖果的库存如下：圆形苹果味 7 个、圆形桃子味 9 个、圆形西瓜味 8 个；星形苹果味 7 个、星形桃子味 6 个、星形西瓜味 4 个。
你可以在取出前凭手感选择圆形或星形，但不能在袋内辨认口味。取出不放回。你要预先确定两种形状各取多少个，使任何可能的取出结果都包含这样一对糖：一颗苹果味、一颗桃子味，并且两颗形状不同。
求能保证目标的最小总取出数量。只输出一个 JSON 对象，唯一字段 answer 的值为整数，不要解释、代码块或其他字段。`;

export const animationPrompt = `创作一个完整的单文件 HTML 动画：夜晚海面上，一艘小船经过灯塔。画面需要能看出船体、灯塔与海浪；灯塔光束扫过海面，海浪循环起伏，船上的小旗随风摆动。动作应连续、协调，窄屏仍能完整观看。
仅使用内联 SVG 和内嵌 CSS，所有图形自行绘制。禁止 JavaScript、事件属性、外链、网络请求、外部字体、嵌入网页和表单。使用 CSS 动画或 SVG 声明式动画。只返回以 <!doctype html> 开头的完整 HTML，不要 Markdown、说明或测试代码。`;

export type QualityMethod = "candy" | "fingerprint" | "animation";
export type QualityVerdict =
  | "passed"
  | "failed"
  | "inconclusive"
  | "needs_review"
  | "reference"
  | "error"
  | "cancelled";
export interface QualityCheckResult {
  method: QualityMethod;
  round: number;
  verdict: QualityVerdict;
  summary: string;
  answer?: number;
  topModel?: string;
  probability?: number;
  review?: Record<string, boolean>;
}
export interface QualityConditions {
  targetId: string;
  targetLabel: string;
  model: string;
  effort: string;
  transport: "codex-account" | "relay";
  protocol: string | null;
  sourceStamp: string | null;
  suite: string;
  fingerprintBank: string | null;
  fingerprintReference: string | null;
  methods: QualityMethod[];
  rounds: number;
}
export interface QualitySampleRecord {
  method: QualityMethod;
  round: number;
  index: number;
  status: "completed" | "error" | "cancelled";
  elapsedMs: number;
  declaredModel: string | null;
  inputTokens: number | null;
  outputTokens: number | null;
  reasoningTokens: number | null;
  cacheReadTokens: number | null;
  firstTokenMs: number | null;
  protocol: string | null;
}
/** 仅保存分项判断和条件；回复、HTML、密钥和服务端错误正文留在本轮内存。 */
export interface QualityBatchReport {
  version: 2;
  id: string;
  at: string;
  conditions: QualityConditions;
  plannedRequests: number;
  attemptedRequests: number;
  state: "completed" | "cancelled" | "error";
  results: QualityCheckResult[];
  samples: QualitySampleRecord[];
}

export function inspectCandyAnswer(raw: string): QualityCheckResult {
  // 只识别约定的最终对象；不从解释、否定句、多个答案或重复键中猜答案。
  const matched = /^\{\s*"answer"\s*:\s*(-?(?:0|[1-9]\d*))\s*\}$/.exec(
    raw.trim(),
  );
  if (!matched || !Number.isSafeInteger(Number(matched[1])))
    return {
      method: "candy",
      round: 1,
      verdict: "inconclusive",
      summary: "未返回唯一整数答案 JSON；本次无法计分。",
    };
  const answer = Number(matched[1]);
  return {
    method: "candy",
    round: 1,
    verdict: answer === 21 ? "passed" : "failed",
    answer,
    summary:
      answer === 21
        ? "最终答案 21，单题通过。"
        : `最终答案 ${answer}，本题未通过（期望 21）。`,
  };
}

export const ANIMATION_REVIEW = [
  { id: "scene", label: "能辨认灯塔、小船与海浪" },
  { id: "motion", label: "光束、海浪、旗帜均持续运动" },
  { id: "continuity", label: "观察至少两个循环，动作连续且互相协调" },
  { id: "layout", label: "在宽屏和窄屏查看，主体没有裁切或遮挡" },
] as const;

export interface AnimationInspection {
  ok: boolean;
  issues: string[];
  html: string | null;
}

export function inspectAnimationHtml(raw: string): AnimationInspection {
  const issues: string[] = [];
  if (raw.length > 250_000)
    return { ok: false, issues: ["动画超过 250 KB，未创建预览。"], html: null };
  if (!/^\s*<!doctype\s+html\s*>/i.test(raw) || !/<\/html>\s*$/i.test(raw))
    issues.push("需要完整 HTML 文档，不能附带 Markdown 或解释。");
  const markup = raw
    .replace(/<meta\s+charset\s*=\s*(["'])utf-?8\1\s*\/?>/gi, "")
    .replace(
      /<meta\s+name\s*=\s*(["'])viewport\1\s+content\s*=\s*(["'])[\w\s.,=+-]+\2\s*\/?>/gi,
      "",
    );
  if (
    /<\s*(script|iframe|frame|object|embed|link|base|meta|form|input|button|textarea|select|audio|video|source|foreignObject|a)\b/i.test(
      markup,
    )
  )
    issues.push("包含脚本、嵌入、导航或交互元素。");
  // SVG 自身的标准命名空间和文档内渐变／形状引用不产生联网。
  const references = raw
    .replace(
      /\sxmlns(?::xlink)?\s*=\s*(["'])http:\/\/www\.w3\.org\/(?:2000\/svg|1999\/xlink)\1/gi,
      "",
    )
    .replace(/\b(?:href|xlink:href)\s*=\s*(["'])#[\w:.-]+\1/gi, "")
    .replace(/url\(\s*(["']?)#[\w:.-]+\1\s*\)/gi, "");
  if (
    /\bon\w+\s*=|\b(?:src|href|xlink:href|action|formaction)\s*=/i.test(
      references,
    )
  )
    issues.push("包含事件属性或资源／链接引用。");
  if (/@import\b|url\s*\(|(?:https?:)?\/\/|javascript\s*:/i.test(references))
    issues.push("包含外部资源、URL 或脚本地址。");
  if (!/<svg\b/i.test(raw)) issues.push("缺少内联 SVG 图形。");
  if (!/@keyframes\s|<animate(?:Transform|Motion)?\b/i.test(raw))
    issues.push("未发现 CSS 或 SVG 动画定义。");
  return { ok: issues.length === 0, issues, html: issues.length ? null : raw };
}

/** 必须配合 iframe sandbox="" 使用。CSP 放在任何模型输出之前。 */
export function animationPreviewDocument(html: string): string {
  const inspection = inspectAnimationHtml(html);
  if (!inspection.ok) throw new Error(inspection.issues.join(" "));
  return `<!doctype html><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'"><meta name="referrer" content="no-referrer">${html.replace(/^\s*<!doctype\s+html\s*>/i, "")}`;
}

export function plannedQualityRequests(
  methods: QualityMethod[],
  rounds: number,
): number {
  if (!Number.isInteger(rounds) || rounds < 1 || rounds > 3)
    throw new Error("测试轮数应为 1–3。");
  return (
    rounds *
    [...new Set(methods)].reduce(
      (n, method) => n + (method === "fingerprint" ? 3 : 1),
      0,
    )
  );
}

export function sameQualityConditions(
  a: QualityConditions,
  b: QualityConditions,
): boolean {
  return (
    typeof a.sourceStamp === "string" &&
    a.sourceStamp.length > 0 &&
    a.sourceStamp === b.sourceStamp &&
    a.protocol !== null &&
    a.targetId === b.targetId &&
    a.transport === b.transport &&
    a.protocol === b.protocol &&
    a.model === b.model &&
    a.effort === b.effort &&
    a.suite === b.suite &&
    a.fingerprintBank === b.fingerprintBank &&
    a.fingerprintReference === b.fingerprintReference &&
    a.rounds === b.rounds &&
    [...a.methods].sort().join(",") === [...b.methods].sort().join(",")
  );
}

export function loadQualityReports(
  storage: Pick<Storage, "getItem">,
): QualityBatchReport[] {
  try {
    const parsed: unknown = JSON.parse(
      storage.getItem(QUALITY_HISTORY_KEY) ?? "[]",
    );
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((x): x is QualityBatchReport => {
        if (
          !x ||
          typeof x !== "object" ||
          x.version !== 2 ||
          typeof x.id !== "string" ||
          typeof x.at !== "string"
        )
          return false;
        const c = x.conditions;
        const token = (value: unknown) =>
          value === null ||
          (typeof value === "number" && Number.isFinite(value) && value >= 0);
        return (
          c &&
          typeof c.targetId === "string" &&
          typeof c.targetLabel === "string" &&
          typeof c.model === "string" &&
          typeof c.effort === "string" &&
          typeof c.suite === "string" &&
          ["codex-account", "relay"].includes(c.transport) &&
          (c.protocol === null || typeof c.protocol === "string") &&
          (c.sourceStamp === undefined ||
            c.sourceStamp === null ||
            typeof c.sourceStamp === "string") &&
          (c.fingerprintBank === null ||
            typeof c.fingerprintBank === "string") &&
          (c.fingerprintReference === null ||
            typeof c.fingerprintReference === "string") &&
          Array.isArray(c.methods) &&
          c.methods.every((m: unknown) =>
            ["candy", "fingerprint", "animation"].includes(String(m)),
          ) &&
          Number.isInteger(c.rounds) &&
          c.rounds >= 1 &&
          c.rounds <= 3 &&
          ["completed", "cancelled", "error"].includes(x.state) &&
          Number.isInteger(x.plannedRequests) &&
          x.plannedRequests >= 1 &&
          x.plannedRequests <= 15 &&
          Number.isInteger(x.attemptedRequests) &&
          x.attemptedRequests >= 0 &&
          x.attemptedRequests <= x.plannedRequests &&
          Array.isArray(x.samples) &&
          x.samples.every(
            (s: QualitySampleRecord) =>
              s &&
              ["candy", "fingerprint", "animation"].includes(s.method) &&
              ["completed", "error", "cancelled"].includes(s.status) &&
              Number.isInteger(s.round) &&
              s.round >= 1 &&
              s.round <= c.rounds &&
              Number.isInteger(s.index) &&
              s.index >= 1 &&
              s.index <= 3 &&
              typeof s.elapsedMs === "number" &&
              Number.isFinite(s.elapsedMs) &&
              s.elapsedMs >= 0 &&
              (s.declaredModel === null ||
                typeof s.declaredModel === "string") &&
              token(s.inputTokens) &&
              token(s.outputTokens) &&
              token(s.reasoningTokens) &&
              token(s.cacheReadTokens) &&
              token(s.firstTokenMs) &&
              (s.protocol === null || typeof s.protocol === "string"),
          ) &&
          Array.isArray(x.results) &&
          x.results.every(
            (r: QualityCheckResult) =>
              r &&
              ["candy", "fingerprint", "animation"].includes(r.method) &&
              Number.isInteger(r.round) &&
              [
                "passed",
                "failed",
                "inconclusive",
                "needs_review",
                "reference",
                "error",
                "cancelled",
              ].includes(r.verdict) &&
              typeof r.summary === "string" &&
              (r.answer === undefined || Number.isSafeInteger(r.answer)) &&
              (r.topModel === undefined || typeof r.topModel === "string") &&
              (r.probability === undefined ||
                (typeof r.probability === "number" &&
                  Number.isFinite(r.probability) &&
                  r.probability >= 0 &&
                  r.probability <= 1)),
          )
        );
      })
      .slice(0, 30)
      .map((report) => ({
        ...report,
        conditions: {
          ...report.conditions,
          sourceStamp: report.conditions.sourceStamp || null,
        },
      }));
  } catch {
    return [];
  }
}

export function saveQualityReports(
  storage: Pick<Storage, "setItem">,
  reports: QualityBatchReport[],
): void {
  storage.setItem(QUALITY_HISTORY_KEY, JSON.stringify(reports.slice(0, 30)));
}
