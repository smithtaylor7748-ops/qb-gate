// 把 cc-switch 的预设（前端 `src/config/*ProviderPresets.ts`）转成 QB Gate 的预设数据文件
// （`crates/qb-relay/src/relay/presets/*.json`，Rust 用 include_str! 读入，前端和以后的深链共用这一份）。
//
// 用法：node scripts/import-ccswitch-presets.mjs <cc-switch 源码目录>
//
// ⛔ 只做**静态**解析：用 TypeScript 编译器把源码读成语法树，只求值字面量、顶层常量和下面认得的几个
// 帮手函数（照 cc-switch 原文重写在这里）。**不执行**下载下来的代码 —— 认不出来的写法就跳过那一条、
// 在输出里列出来，不猜。
//
// 筛选（使用者 2026-10-25 定的）：
// - 只要「官方类」：official / cn_official / cloud_provider / local；聚合平台、第三方中转一律不搬
//   （QB Gate 的规矩：预设只放厂商官方端点，怕使用者把第三方地址当成推荐）；
// - 标了商业合作伙伴（isPartner）的整条不搬；隐藏的、要 OAuth 登录的不搬；没有固定地址的（要填区域模板、
//   走云平台凭据）也不搬 —— 它们进不了本机路由；
// - 网址里的推广参数（aff / ref / invite* / ch / track_id / utm_*）一律剥掉。
//
// cc-switch v4.0.14（https://github.com/farion1231/cc-switch，提交 5de6feb4…），MIT，Copyright (c) 2025 Jason Young。
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import ts from "typescript";

const COMMIT = "5de6feb4e5adda61f4ad824ce0df2c449245f447";
const KEEP = new Set(["official", "cn_official", "cloud_provider", "local"]);
const PROMO =
  /^(aff|ref|invite|invitecode|invite_code|invitationcode|ch|track_id|utm_.+)$/i;
const ROLE_IDS = {
  sonnet: "claude-sonnet-5",
  opus: "claude-opus-5",
  fable: "claude-fable-5",
  haiku: "claude-haiku-4-5",
};

const root = process.argv[2];
if (!root) {
  console.error(
    "用法：node scripts/import-ccswitch-presets.mjs <cc-switch 源码目录>",
  );
  process.exit(2);
}
const outDir = resolve("crates/qb-relay/src/relay/presets");
mkdirSync(outDir, { recursive: true });

class Unsupported extends Error {}

// ------------------------------------------------------------------ 帮手函数（照 cc-switch 原文）

const isClaudeHaiku4 = (model) => /claude-haiku-4[-.]/i.test(model);
const helpers = {
  generateThirdPartyAuth: (apiKey) => ({ OPENAI_API_KEY: apiKey || "" }),
  generateThirdPartyConfig: (providerName, baseUrl, modelName, options) => ({
    __codexConfig: true,
    providerName,
    baseUrl,
    model: modelName ?? "gpt-5.6-sol",
    requiresOpenAiAuth: options?.requiresOpenAiAuth ?? true,
  }),
  modelCatalog: (models) =>
    models.map((entry) =>
      typeof entry === "string" ? { model: entry } : { ...entry },
    ),
  passthroughRoutes: (supports1m = false) => [
    { role: "sonnet", upstream: ROLE_IDS.sonnet, supports1m },
    { role: "opus", upstream: ROLE_IDS.opus, supports1m },
    {
      role: "haiku",
      upstream: ROLE_IDS.haiku,
      supports1m: supports1m && !isClaudeHaiku4(ROLE_IDS.haiku),
    },
  ],
  mappedRoutes: (sonnet, opus, haiku, supports1m = false) => [
    { role: "sonnet", upstream: sonnet, supports1m },
    { role: "opus", upstream: opus, supports1m },
    {
      role: "haiku",
      upstream: haiku,
      supports1m: supports1m && !isClaudeHaiku4(haiku),
    },
  ],
  brandedRoutes: (sonnet, opus, haiku, supports1m = false) => {
    const seen = new Set();
    return [
      { role: "sonnet", upstream: sonnet },
      { role: "opus", upstream: opus },
      { role: "haiku", upstream: haiku },
    ]
      .map((r) => ({ ...r, supports1m }))
      .filter((r) => !seen.has(r.upstream) && seen.add(r.upstream));
  },
};

// ------------------------------------------------------------------ 静态求值

function evaluator(sf) {
  const consts = new Map();
  for (const st of sf.statements) {
    if (!ts.isVariableStatement(st)) continue;
    for (const d of st.declarationList.declarations) {
      if (ts.isIdentifier(d.name) && d.initializer)
        consts.set(d.name.text, d.initializer);
    }
  }
  const memo = new Map();
  const name = (n) => {
    if (ts.isIdentifier(n) || ts.isPrivateIdentifier(n)) return n.text;
    if (ts.isStringLiteral(n) || ts.isNumericLiteral(n)) return n.text;
    if (ts.isComputedPropertyName(n)) return String(ev(n.expression));
    throw new Unsupported(`属性名 ${ts.SyntaxKind[n.kind]}`);
  };
  const ev = (n) => {
    if (
      ts.isAsExpression(n) ||
      ts.isSatisfiesExpression(n) ||
      ts.isParenthesizedExpression(n) ||
      ts.isTypeAssertionExpression(n) ||
      ts.isNonNullExpression(n)
    )
      return ev(n.expression);
    if (ts.isStringLiteral(n) || ts.isNoSubstitutionTemplateLiteral(n))
      return n.text;
    if (ts.isNumericLiteral(n)) return Number(n.text);
    if (n.kind === ts.SyntaxKind.TrueKeyword) return true;
    if (n.kind === ts.SyntaxKind.FalseKeyword) return false;
    if (n.kind === ts.SyntaxKind.NullKeyword) return null;
    if (ts.isIdentifier(n) && n.text === "undefined") return undefined;
    if (
      ts.isPrefixUnaryExpression(n) &&
      n.operator === ts.SyntaxKind.MinusToken
    )
      return -ev(n.operand);
    if (ts.isTemplateExpression(n))
      return (
        n.head.text +
        n.templateSpans
          .map((s) => String(ev(s.expression)) + s.literal.text)
          .join("")
      );
    if (
      ts.isBinaryExpression(n) &&
      n.operatorToken.kind === ts.SyntaxKind.PlusToken
    )
      return ev(n.left) + ev(n.right);
    if (ts.isArrayLiteralExpression(n)) {
      const out = [];
      for (const e of n.elements) {
        if (ts.isSpreadElement(e)) out.push(...ev(e.expression));
        else out.push(ev(e));
      }
      return out;
    }
    if (ts.isObjectLiteralExpression(n)) {
      const out = {};
      for (const p of n.properties) {
        if (ts.isPropertyAssignment(p)) {
          const v = ev(p.initializer);
          if (v !== undefined) out[name(p.name)] = v;
        } else if (ts.isShorthandPropertyAssignment(p)) {
          out[p.name.text] = ev(p.name);
        } else if (ts.isSpreadAssignment(p)) {
          Object.assign(out, ev(p.expression));
        } else {
          throw new Unsupported(`对象成员 ${ts.SyntaxKind[p.kind]}`);
        }
      }
      return out;
    }
    if (ts.isPropertyAccessExpression(n)) {
      const obj = ev(n.expression);
      if (obj == null || typeof obj !== "object")
        throw new Unsupported(`取属性 ${n.getText(sf)}`);
      return obj[n.name.text];
    }
    if (ts.isCallExpression(n) && ts.isIdentifier(n.expression)) {
      const fn = helpers[n.expression.text];
      if (!fn) throw new Unsupported(`调用 ${n.expression.text}()`);
      return fn(...n.arguments.map(ev));
    }
    if (ts.isIdentifier(n)) {
      if (n.text in ROLE_IDS) return ROLE_IDS[n.text];
      if (n.text === "CLAUDE_DESKTOP_ROLE_ROUTE_IDS") return ROLE_IDS;
      if (!consts.has(n.text)) throw new Unsupported(`标识符 ${n.text}`);
      if (!memo.has(n.text)) memo.set(n.text, ev(consts.get(n.text)));
      return memo.get(n.text);
    }
    throw new Unsupported(ts.SyntaxKind[n.kind]);
  };
  return { ev, consts };
}

// ------------------------------------------------------------------ 清理与映射

function cleanUrl(raw) {
  if (typeof raw !== "string" || !raw) return null;
  try {
    const u = new URL(raw);
    for (const k of [...u.searchParams.keys()])
      if (PROMO.test(k)) u.searchParams.delete(k);
    if (u.hash.includes("?")) {
      const [path, query] = u.hash.slice(1).split("?", 2);
      const q = new URLSearchParams(query);
      for (const k of [...q.keys()]) if (PROMO.test(k)) q.delete(k);
      const rest = q.toString();
      u.hash = rest ? `${path}?${rest}` : path;
    }
    let out = u.toString();
    if (out.endsWith("#/") || out.endsWith("#")) out = out.replace(/#\/?$/, "");
    if (
      !raw.endsWith("/") &&
      out.endsWith("/") &&
      u.pathname === "/" &&
      !u.search &&
      !u.hash
    )
      out = out.slice(0, -1);
    return out;
  } catch {
    return raw;
  }
}

const ONE_M = /\[1m\]$/i;
const stripOneM = (m) =>
  typeof m === "string" ? m.replace(ONE_M, "").trim() : null;
const nonEmpty = (v) => (typeof v === "string" && v.trim() ? v.trim() : null);

function slug(name, baseUrl, taken) {
  let base = String(name)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  if (!base) {
    try {
      base = new URL(baseUrl).hostname
        .replace(/^(api|www)\./, "")
        .split(".")[0];
    } catch {
      base = "preset";
    }
  }
  let id = base;
  for (let i = 2; taken.has(id); i++) id = `${base}-${i}`;
  taken.add(id);
  return id;
}

function fixedHttpUrl(u) {
  return typeof u === "string" && /^https?:\/\//.test(u) && !/[{}$]/.test(u);
}

function common(p) {
  return {
    name: p.name,
    category: p.category,
    website: cleanUrl(p.websiteUrl),
    api_key_url: cleanUrl(p.apiKeyUrl),
  };
}

// Claude Code：settingsConfig.env 里的地址和各档模型。
function claudeCode(p) {
  const env = p.settingsConfig?.env ?? {};
  const base = env.ANTHROPIC_BASE_URL;
  if (!fixedHttpUrl(base)) return { skip: "没有固定地址" };
  const extra = {};
  for (const [k, v] of Object.entries(env)) {
    if (k.startsWith("ANTHROPIC_") || k === "CLAUDE_CODE_SUBAGENT_MODEL")
      continue;
    if (typeof v === "string") extra[k] = v;
    else if (typeof v === "number" || typeof v === "boolean")
      extra[k] = String(v);
  }
  const models = [
    env.ANTHROPIC_MODEL,
    env.ANTHROPIC_DEFAULT_HAIKU_MODEL,
    env.ANTHROPIC_DEFAULT_SONNET_MODEL,
    env.ANTHROPIC_DEFAULT_OPUS_MODEL,
    env.ANTHROPIC_DEFAULT_FABLE_MODEL,
  ];
  return {
    ...common(p),
    base_url: base.trim(),
    api_format: p.apiFormat ?? "anthropic",
    auth_field:
      p.apiKeyField ??
      ("ANTHROPIC_API_KEY" in env
        ? "ANTHROPIC_API_KEY"
        : "ANTHROPIC_AUTH_TOKEN"),
    models: {
      default: stripOneM(nonEmpty(env.ANTHROPIC_MODEL)),
      haiku: stripOneM(nonEmpty(env.ANTHROPIC_DEFAULT_HAIKU_MODEL)),
      sonnet: stripOneM(nonEmpty(env.ANTHROPIC_DEFAULT_SONNET_MODEL)),
      opus: stripOneM(nonEmpty(env.ANTHROPIC_DEFAULT_OPUS_MODEL)),
      fable: stripOneM(nonEmpty(env.ANTHROPIC_DEFAULT_FABLE_MODEL)),
      subagent: stripOneM(nonEmpty(env.CLAUDE_CODE_SUBAGENT_MODEL)),
    },
    supports_1m: models.some((m) => typeof m === "string" && ONE_M.test(m)),
    catalog: [],
    endpoint_candidates: (p.endpointCandidates ?? []).filter(fixedHttpUrl),
    models_url: nonEmpty(p.modelsUrl),
    extra_env: extra,
  };
}

// Claude 桌面端：baseUrl + 三档路由。
function claudeDesktop(p) {
  if (!fixedHttpUrl(p.baseUrl)) return { skip: "没有固定地址" };
  const role = (r) =>
    p.modelRoutes?.find((x) => x.role === r)?.upstream ?? null;
  return {
    ...common(p),
    base_url: p.baseUrl.trim(),
    api_format: p.apiFormat ?? "anthropic",
    auth_field: p.apiKeyField ?? "ANTHROPIC_AUTH_TOKEN",
    models: {
      default: null,
      haiku: stripOneM(role("haiku")),
      sonnet: stripOneM(role("sonnet")),
      opus: stripOneM(role("opus")),
      fable: stripOneM(role("fable")),
      subagent: null,
    },
    supports_1m: (p.modelRoutes ?? []).some((r) => r.supports1m),
    catalog: [],
    endpoint_candidates: (p.endpointCandidates ?? []).filter(fixedHttpUrl),
    models_url: null,
    extra_env: {},
  };
}

// Codex：generateThirdPartyConfig 的参数 + 模型目录。
function codex(p) {
  const c = p.config;
  if (!c || !c.__codexConfig)
    return { skip: "config 不是 generateThirdPartyConfig 生成的" };
  if (!fixedHttpUrl(c.baseUrl)) return { skip: "没有固定地址" };
  return {
    ...common(p),
    base_url: c.baseUrl.trim(),
    api_format: p.apiFormat ?? "openai_responses",
    auth_field: null,
    models: {
      default: nonEmpty(c.model),
      haiku: null,
      sonnet: null,
      opus: null,
      fable: null,
      subagent: null,
    },
    supports_1m: false,
    catalog: (p.modelCatalog ?? []).map((m) => ({
      model: m.model,
      display_name: nonEmpty(m.displayName),
      context_window:
        typeof m.contextWindow === "number" ? m.contextWindow : null,
    })),
    endpoint_candidates: (p.endpointCandidates ?? []).filter(fixedHttpUrl),
    models_url: null,
    extra_env: {},
  };
}

const FILES = [
  [
    "claudeProviderPresets.ts",
    "providerPresets",
    "claude-code.json",
    claudeCode,
  ],
  [
    "claudeDesktopProviderPresets.ts",
    "claudeDesktopProviderPresets",
    "claude-desktop.json",
    claudeDesktop,
  ],
  ["codexProviderPresets.ts", "codexProviderPresets", "codex.json", codex],
];

for (const [file, arrayName, out, map] of FILES) {
  const path = join(root, "src", "config", file);
  const sf = ts.createSourceFile(
    path,
    readFileSync(path, "utf8"),
    ts.ScriptTarget.Latest,
    true,
  );
  const { ev, consts } = evaluator(sf);
  const list = consts.get(arrayName);
  if (
    !list ||
    !ts.isArrayLiteralExpression(
      ts.isAsExpression(list) ? list.expression : list,
    )
  )
    throw new Error(`${file} 里找不到预设数组 ${arrayName}`);
  const elements = (ts.isAsExpression(list) ? list.expression : list).elements;
  const kept = [];
  const skipped = [];
  const taken = new Set();
  for (const el of elements) {
    let p;
    try {
      p = ev(el);
    } catch (e) {
      if (!(e instanceof Unsupported)) throw e;
      const label = el.getText(sf).match(/name:\s*"([^"]+)"/)?.[1] ?? "?";
      const category = el.getText(sf).match(/category:\s*"([a-z_]+)"/)?.[1];
      if (category && KEEP.has(category))
        skipped.push(`${label}：认不出 ${e.message}`);
      continue;
    }
    if (!KEEP.has(p.category)) continue;
    if (p.isPartner) continue;
    if (p.hidden || p.requiresOAuth || p.providerType) continue;
    const mapped = map(p);
    if (mapped.skip) {
      skipped.push(`${p.name}：${mapped.skip}`);
      continue;
    }
    // 地址里留着 `YOUR_RESOURCE_NAME` 这种占位（Azure、Cloudflare 的官方端点要填自己的资源名）：留着，标出来让界面提醒。
    mapped.needs_edit = /YOUR_[A-Z_]+/.test(mapped.base_url);
    kept.push({ id: slug(p.name, mapped.base_url, taken), ...mapped });
  }
  const body = {
    source: `cc-switch v4.0.14 src/config/${file}（提交 ${COMMIT}）`,
    license: "MIT, Copyright (c) 2025 Jason Young — 见 ATTRIBUTION.md",
    generated_by: "scripts/import-ccswitch-presets.mjs（静态解析，不执行源码）",
    filter:
      "只要 official / cn_official / cloud_provider / local；不要合作伙伴、隐藏、OAuth、没有固定地址的；网址剥掉推广参数",
    skipped,
    presets: kept,
  };
  writeFileSync(join(outDir, out), JSON.stringify(body, null, 2) + "\n");
  console.log(`${out}: ${kept.length} 条，跳过 ${skipped.length} 条`);
  for (const s of skipped) console.log(`  - ${s}`);
}
