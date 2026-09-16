/**
 * 客户端配置文件里那几个开关和模型映射，**纯逻辑**。
 *
 * 界面在 `features/station/ConfigPane.tsx`；这里一行 React 都没有，
 * 所以每一条判定都能单测。
 *
 * # ⛔ 键名和判定条件全部照 cc-switch 的源码抄
 *
 * cc-switch（MIT，Copyright (c) 2025 Jason Young，见 ATTRIBUTION.md）的
 * `src/components/providers/forms/CommonConfigEditor.tsx` 与
 * `hooks/useModelState.ts`。**自己编一套键名的话，写进去的东西
 * Claude Code 根本不认，而界面上看起来一切正常** —— 那是最糟的一类失败。
 *
 * # ⛔ 读的判定必须精确，不能只看「这个键在不在」
 *
 * 0.17.0 的第一版就是只看在不在。后果：一个把 `CLAUDE_CODE_EFFORT_LEVEL`
 * 设成 `"medium"` 的人，界面上「最大强度思考」是勾着的；他取消勾选，
 * 那一行被整个删掉 —— 他的 `medium` 没了，而他以为自己只是关掉了一个他
 * 本来就没开的开关。**读和写必须是同一个断言的两面。**
 */

// ------------------------------------------------------------------ 六个快捷开关

/** 配置对象。值是什么形状由各个键自己说了算，所以一律 `unknown`。 */
export type ConfigObject = Record<string, unknown>;

export interface QuickSwitch {
  id: string;
  label: string;
  /** 一句话说明它到底改了什么。界面挂在 title 上。 */
  hint: string;
  /** 默认勾上。目前只有「禁用自动升级」。 */
  defaultOn?: boolean;
  /** 这个开关现在是不是开着。**精确判定**，见文件头。 */
  read: (o: ConfigObject) => boolean;
  /** 打开 / 关掉。关掉时删键，`env` 空了连 `env` 一起删。 */
  apply: (o: ConfigObject, on: boolean) => void;
}

function envOf(o: ConfigObject): ConfigObject | null {
  const e = o.env;
  return e && typeof e === "object" && !Array.isArray(e)
    ? (e as ConfigObject)
    : null;
}

export function getEnv(o: ConfigObject, key: string): unknown {
  return envOf(o)?.[key];
}

/**
 * 写一个 `env` 键。`value` 传 `null` = 删掉它。
 *
 * ⛔ **`env` 空了要连 `env` 一起删。** 留一个空的 `env: {}` 不影响客户端运行，
 * 但它会让「跟随表单」算出来的 JSON 跟手写的那份永远不相等，
 * 于是界面一直停在「已手改」——「跟随」这个功能就整个失效了。
 */
export function setEnv(o: ConfigObject, key: string, value: string | null) {
  const env = envOf(o) ?? {};
  if (value === null) delete env[key];
  else env[key] = value;
  if (Object.keys(env).length === 0) delete o.env;
  else o.env = env;
}

/** `"1"` 和 `1` 都算开着 —— 手写 JSON 的人两种都会写。 */
function truthyOne(v: unknown): boolean {
  return v === "1" || v === 1;
}

export const QUICK_SWITCHES: QuickSwitch[] = [
  {
    id: "hideAttribution",
    label: "隐藏 AI 署名",
    hint: "提交信息和 PR 描述里不再带 Claude 的署名。写 attribution:{commit:'',pr:''}",
    // ⛔ 两项都必须是空串。只判「attribution 在不在」的话，一个真的配了
    // 署名模板的人会被显示成「已隐藏」，取消勾选把他的模板整个删掉。
    read: (o) => {
      const a = o.attribution;
      if (!a || typeof a !== "object") return false;
      const r = a as ConfigObject;
      return r.commit === "" && r.pr === "";
    },
    apply: (o, on) => {
      if (on) o.attribution = { commit: "", pr: "" };
      else delete o.attribution;
    },
  },
  {
    id: "teammates",
    label: "Teammates 模式",
    hint: "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1 —— 实验特性，上游随时可能改",
    read: (o) => truthyOne(getEnv(o, "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS")),
    apply: (o, on) =>
      setEnv(o, "CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", on ? "1" : null),
  },
  {
    id: "enableToolSearch",
    label: "启用 Tool Search",
    hint: "ENABLE_TOOL_SEARCH=true。写的是 true，但 1 也认",
    // 写 "true"，读时 "1" 也算 —— 跟 cc-switch 一致：手写的人两种都会写。
    read: (o) => {
      const v = getEnv(o, "ENABLE_TOOL_SEARCH");
      return v === "true" || v === "1";
    },
    apply: (o, on) => setEnv(o, "ENABLE_TOOL_SEARCH", on ? "true" : null),
  },
  {
    id: "effortMax",
    label: "最大强度思考",
    hint: "CLAUDE_CODE_EFFORT_LEVEL=max。⛔ 只有 max 才算开着 —— 设成别的档位不会被当成开",
    // ⛔ 严格等于 "max"。设成 "medium" 的人不该看到这个勾是开的，
    // 否则他一取消勾选就把自己的设置删掉了。
    read: (o) => getEnv(o, "CLAUDE_CODE_EFFORT_LEVEL") === "max",
    apply: (o, on) => setEnv(o, "CLAUDE_CODE_EFFORT_LEVEL", on ? "max" : null),
  },
  {
    id: "disableAutoUpgrade",
    label: "禁用自动升级",
    hint: "DISABLE_AUTOUPDATER=1。⛔ 默认开着 —— 客户端自己升级出来的新副本没有 Deny ACE，是现成的绕过入口",
    defaultOn: true,
    read: (o) => truthyOne(getEnv(o, "DISABLE_AUTOUPDATER")),
    apply: (o, on) => setEnv(o, "DISABLE_AUTOUPDATER", on ? "1" : null),
  },
  {
    id: "disableArtifact",
    label: "禁用 Artifact 工具",
    // 这句话是 cc-switch 源码里那段注释说的，照搬 —— 它解释了这个开关
    // 为什么存在，而不是「有这么个变量所以放上来」。
    hint: "CLAUDE_CODE_DISABLE_ARTIFACT=1。第三方网关（如 DeepSeek）用严格 JSON Schema 校验工具定义，Artifact 里的 \\p{..} 正则会让每个请求 400",
    read: (o) => truthyOne(getEnv(o, "CLAUDE_CODE_DISABLE_ARTIFACT")),
    apply: (o, on) =>
      setEnv(o, "CLAUDE_CODE_DISABLE_ARTIFACT", on ? "1" : null),
  },
];

// ------------------------------------------------------------------ 1M 上下文标记

/**
 * 声明这个模型支持 1M 上下文的写法：**模型名末尾加一个后缀**，不是另一个键。
 *
 * ⛔ 查不到「1M」对应的环境变量是有原因的 —— 根本没有那个变量。
 * cc-switch 把它编码进模型名里（`CLAUDE_ONE_M_MARKER`），中转站那边据此
 * 决定要不要带上 1M 的 beta 头。当初没查证就不敢做这一列，是对的。
 */
export const ONE_M_MARKER = "[1M]";

/** 大小写不敏感 —— 手写的人会写 `[1m]`。 */
export function hasOneM(model: string): boolean {
  return model.trimEnd().toLowerCase().endsWith("[1m]");
}

export function stripOneM(model: string): string {
  const t = model.trimEnd();
  if (!t.toLowerCase().endsWith("[1m]")) return model;
  return t.slice(0, -ONE_M_MARKER.length).trimEnd();
}

/** 空模型名加不了标记 —— 返回空串，让调用方把这个键删掉。 */
export function setOneM(model: string, on: boolean): string {
  const base = stripOneM(model).trim();
  if (!base) return "";
  return on ? `${base}${ONE_M_MARKER}` : base;
}

// ------------------------------------------------------------------ 模型映射

export interface ModelRole {
  /** 环境变量名：实际请求用哪个模型。 */
  key: string;
  /** 菜单里显示成什么。`null` = 这一档没有显示名这个概念。 */
  nameKey: string | null;
  label: string;
  hint: string;
  /** 能不能标 1M。主模型和子代理那两档 cc-switch 没给这个开关。 */
  oneM: boolean;
}

/**
 * ⛔ **`ANTHROPIC_SMALL_FAST_MODEL` 是旧键，不要再写。**
 *
 * cc-switch 每次写模型时都会主动 `delete env.ANTHROPIC_SMALL_FAST_MODEL`，
 * 新键是 `CLAUDE_CODE_SUBAGENT_MODEL`。两个都留着的话，客户端读哪个取决于
 * 它自己的优先级 —— 而那个优先级我们没法从这里看出来，于是「我明明改了，
 * 子代理还在走旧模型」变成一个查不出来的问题。
 */
export const LEGACY_SUBAGENT_KEY = "ANTHROPIC_SMALL_FAST_MODEL";

export const MODEL_ROLES: ModelRole[] = [
  {
    key: "ANTHROPIC_MODEL",
    nameKey: null,
    label: "主模型",
    hint: "不写就用客户端自己的默认",
    oneM: false,
  },
  {
    key: "ANTHROPIC_DEFAULT_OPUS_MODEL",
    nameKey: "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
    label: "Opus 档",
    hint: "/model opus 选到的那个",
    oneM: true,
  },
  {
    key: "ANTHROPIC_DEFAULT_SONNET_MODEL",
    nameKey: "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
    label: "Sonnet 档",
    hint: "/model sonnet 选到的那个",
    oneM: true,
  },
  {
    key: "ANTHROPIC_DEFAULT_FABLE_MODEL",
    nameKey: "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
    label: "Fable 档",
    hint: "/model fable 选到的那个",
    oneM: true,
  },
  {
    key: "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    nameKey: "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
    label: "Haiku 档",
    hint: "/model haiku 选到的那个",
    oneM: true,
  },
  {
    key: "CLAUDE_CODE_SUBAGENT_MODEL",
    nameKey: null,
    label: "子代理",
    hint: "标题、摘要这类小活儿走它 —— 中转站上这一档往往便宜得多",
    oneM: false,
  },
];

/** 从配置里读出当前的模型映射。缺的写空串。 */
export function readModels(o: ConfigObject): Record<string, string> {
  const out: Record<string, string> = {};
  for (const r of MODEL_ROLES) {
    const v = getEnv(o, r.key);
    out[r.key] = typeof v === "string" ? v : "";
    if (r.nameKey) {
      const n = getEnv(o, r.nameKey);
      out[r.nameKey] = typeof n === "string" ? n : "";
    }
  }
  return out;
}

/**
 * 把模型映射写回配置。空串 = 删键（**不写空值**）。
 *
 * ⛔ 写空值的话客户端会拿空字符串当模型名去请求，上游回一个看不懂的 400。
 */
export function applyModels(o: ConfigObject, map: Record<string, string>) {
  for (const r of MODEL_ROLES) {
    setEnv(o, r.key, (map[r.key] ?? "").trim() || null);
    if (r.nameKey) setEnv(o, r.nameKey, (map[r.nameKey] ?? "").trim() || null);
  }
  // 旧键每次都删 —— 见 LEGACY_SUBAGENT_KEY。
  setEnv(o, LEGACY_SUBAGENT_KEY, null);
}

// ------------------------------------------------------------------ JSON 校验

/**
 * JSON 错在哪一类。
 *
 * ⛔ 只说「格式错误」等于没说 —— 人还得自己一行行找。这四类是完全不同的
 * 手误，各有各的找法：尾逗号往回看一行，键没引号是整段风格问题，
 * 括号不配对要数括号，值没引号通常是粘了一个裸的域名进去。
 */
export function jsonProblem(text: string): string | null {
  try {
    JSON.parse(text);
    return null;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    if (/,\s*[}\]]/.test(text))
      return "有尾逗号 —— 最后一项后面那个逗号要去掉（JSON 不允许，JS 允许，所以很容易带进来）";
    if (/[{,]\s*[A-Za-z_$][\w$]*\s*:/.test(text))
      return '键没加引号 —— JSON 的键必须是 "双引号" 字符串，单引号也不行';
    const open = (text.match(/[{[]/g) ?? []).length;
    const close = (text.match(/[}\]]/g) ?? []).length;
    if (open !== close)
      return `括号不配对 —— 开了 ${open} 个、关了 ${close} 个`;
    if (/:\s*[A-Za-z][\w./:-]*\s*[,}\n]/.test(text))
      return '有个值没加引号 —— 字符串值也要 "双引号"（只有数字、true/false/null 可以裸写）';
    return `解析不了：${msg}`;
  }
}

/** 按当前开关与映射算出来的 JSON。手改过的那份不走这条路。 */
export function compose(
  base: string,
  on: Record<string, boolean>,
  models: Record<string, string>,
): string {
  let o: ConfigObject;
  try {
    const v: unknown = JSON.parse(base);
    o =
      v && typeof v === "object" && !Array.isArray(v)
        ? (v as ConfigObject)
        : {};
  } catch {
    o = {};
  }
  for (const s of QUICK_SWITCHES) s.apply(o, Boolean(on[s.id]));
  applyModels(o, models);
  return JSON.stringify(o, null, 2);
}

// ------------------------------------------------------------------ Codex 的 TOML

/**
 * 「顶级字段」到第一个 `[section]` 为止。
 *
 * ⛔ 不能全文找 —— `[model_providers.qb_relay]` 底下也可能有同名键，
 * 改错了地方的症状是「我明明设了，它没生效」。
 */
function topLevelEnd(lines: string[]): number {
  const i = lines.findIndex((l) => /^\s*\[/.test(l));
  return i === -1 ? lines.length : i;
}

function intLine(field: string): RegExp {
  return new RegExp(`^\\s*${field}\\s*=\\s*(\\d+)\\s*(?:#.*)?$`);
}

/** 读一个顶级整数字段。读不到 / 不是整数就是 `undefined`。 */
export function tomlInt(text: string, field: string): number | undefined {
  const lines = text.split("\n");
  const end = topLevelEnd(lines);
  const re = intLine(field);
  for (let i = 0; i < end; i += 1) {
    const m = lines[i].match(re);
    if (m) return Number(m[1]);
  }
  return undefined;
}

/** 写一个顶级整数字段。已有就改，没有就插在顶级区末尾（section 之前）。 */
export function setTomlInt(text: string, field: string, value: number): string {
  const lines = text.split("\n");
  const end = topLevelEnd(lines);
  const re = intLine(field);
  const line = `${field} = ${value}`;
  for (let i = 0; i < end; i += 1) {
    if (re.test(lines[i])) {
      lines[i] = line;
      return lines.join("\n");
    }
  }
  lines.splice(end, 0, line);
  return lines.join("\n");
}

/** 删掉一个顶级整数字段。没有就原样返回。 */
export function removeTomlInt(text: string, field: string): string {
  const lines = text.split("\n");
  const end = topLevelEnd(lines);
  const re = intLine(field);
  for (let i = 0; i < end; i += 1) {
    if (re.test(lines[i])) {
      lines.splice(i, 1);
      return lines.join("\n");
    }
  }
  return text;
}

function strLine(field: string): RegExp {
  return new RegExp(`^\\s*${field}\\s*=\\s*"([^"]*)"\\s*(?:#.*)?$`);
}

/** 读一个顶级字符串字段。 */
export function tomlStr(text: string, field: string): string | undefined {
  const lines = text.split("\n");
  const end = topLevelEnd(lines);
  const re = strLine(field);
  for (let i = 0; i < end; i += 1) {
    const m = lines[i].match(re);
    if (m) return m[1];
  }
  return undefined;
}

/** 写一个顶级字符串字段；`value` 传 `null` 删掉它。 */
export function setTomlStr(
  text: string,
  field: string,
  value: string | null,
): string {
  const lines = text.split("\n");
  const end = topLevelEnd(lines);
  const re = strLine(field);
  for (let i = 0; i < end; i += 1) {
    if (re.test(lines[i])) {
      if (value === null) lines.splice(i, 1);
      else lines[i] = `${field} = "${value}"`;
      return lines.join("\n");
    }
  }
  if (value === null) return text;
  lines.splice(end, 0, `${field} = "${value}"`);
  return lines.join("\n");
}

/** Codex 的 1M 上下文窗口。 */
export const CODEX_ONE_M = 1_000_000;
/**
 * 开 1M 时顺带设的自动压缩上限。
 *
 * ⛔ 两个要一起设。只设上下文窗口的话，客户端会按默认的压缩阈值（远小于 1M）
 * 提前压缩 —— 1M 那一档等于白开，而界面上开关是亮的。
 */
export const CODEX_COMPACT_LIMIT = 900_000;

/** Codex 的思考等级。顺序就是界面上的顺序。 */
export const CODEX_EFFORTS = ["minimal", "low", "medium", "high"] as const;
