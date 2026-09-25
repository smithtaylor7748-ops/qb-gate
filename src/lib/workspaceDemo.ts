/** Public screenshots use synthetic, reserved examples only. No machine state is read. */
// 目录随扩展中心一起搬进了 qb-extensions（W2 拆 crate）。
// demo 数据直接读那一份，免得两边的精选条目对不上。
import catalog from "../../crates/qb-extensions/extension-catalog.json";
import type { Workspace } from "./generated/Workspace";
const state: Workspace = {
  providers: [
    {
      id: "example",
      name: "我的 API 服务",
      base_url: "https://relay.example.com/v1",
      website: "",
      note: "工作使用的模型服务",
      tags: ["工作"],
      favorite: true,
      revision: 1,
      // 没填充值比例 = 按最常见的 1 元 = 1 美元额度算。
      topup_per_usd: null,
    },
    {
      // 第二个站点：中转站页要能看出「一个站点底下挂几个分组」这两层结构，
      // 只有一个站点的话那一层在截图里看不出来。
      id: "moxi",
      name: "备用中转",
      base_url: "https://backup.example.com/anthropic",
      website: "",
      note: "备用",
      tags: [],
      favorite: false,
      revision: 1,
      // 这家按真实汇率卖额度：7 元买 1 美元。它标的 ×0.12 折成每 $1 牌价是 0.84 元，
      // 比 example 那家 1 元一美元、标 ×0.2 的还贵 —— 帖子说的「倍率陷阱」，
      // 中转站页的倍率格与站点标题上要看得出来。
      topup_per_usd: 7,
    },
  ],
  credentials: [
    {
      id: "sample-key",
      provider_id: "example",
      label: "开发凭证",
      masked: "示例凭证",
      available: true,
      revision: 1,
    },
  ],
  environments: [
    {
      id: "sample-code",
      name: "Claude · 开发环境",
      client: "claude-code",
      provider_id: "example",
      credential_id: "sample-key",
      model: "自定义模型",
      small_model: "",
      wire_api: "responses",
      auth_style: "env_key",
      revision: 1,
      applied_revision: 1,
      config_dir:
        "C:\\Users\\demo\\AppData\\Local\\ClaudeIpGate\\environments\\sample-code",
      config_state: "applied",
      via_router: false,
    },
  ],
  sessions: [],
  operations: [],
  installations: [],
  launch_plans: [],
  migration_notes: [],
};
export async function demoWorkspaceCall(
  command: string,
  args: Record<string, unknown> = {},
): Promise<unknown> {
  if (command === "workspace_state") return structuredClone(state);
  if (command === "extension_catalog") return catalog;
  if (command === "diagnostic_history") return [];
  if (command === "workspace_references") return [];
  if (command === "environment_preview")
    return [
      {
        path: state.environments[0].config_dir + "\\settings.json",
        before: "{}",
        after:
          '{\n  "env": { "ANTHROPIC_BASE_URL": "https://relay.example.com" }\n}',
      },
    ];
  if (command === "extension_preview")
    return {
      destination: "C:\\Users\\demo\\.claude\\skills",
      files: ["SKILL.md"],
      changes: ["添加 SKILL.md"],
      conflict: false,
    };
  // 中转站的添加弹窗会顺手建站点（站点名撞上已有的就复用）。
  // ⛔ 演示里也要真的加进列表：只回一个 id 的话，加完线路看不见它属于哪个站，
  // 而那正好是「复用还是新建」这条分支唯一看得出来的地方。
  if (command === "provider_save") {
    const p = args.provider as Workspace["providers"][number];
    const saved = { ...p, id: p.id || crypto.randomUUID(), revision: 1 };
    const at = state.providers.findIndex((x) => x.id === saved.id);
    if (at >= 0) state.providers[at] = saved;
    else state.providers.push(saved);
    return saved;
  }
  if (command === "credential_save") {
    const c = args.credential as Workspace["credentials"][number];
    const saved = {
      ...c,
      id: c.id || crypto.randomUUID(),
      available: true,
      masked: "***",
      revision: c.revision + 1,
    };
    const at = state.credentials.findIndex((x) => x.id === saved.id);
    if (at >= 0) state.credentials[at] = saved;
    else state.credentials.push(saved);
    return saved;
  }
  if (command === "session_launch") {
    const session = {
      id: crypto.randomUUID(),
      context: {
        client: args.client,
        identity_kind: args.kind,
        identity_id: args.id,
        config_dir: "演示目录",
        working_dir: "演示工作目录",
      },
      pid: 1000,
      process_created: "1",
      started_at: new Date().toISOString(),
      state: "running",
      gated: true,
      detail: "演示会话",
      config_revision: 1,
    };
    state.sessions.push(session as Workspace["sessions"][number]);
    return session;
  }
  if (command === "session_stop") {
    const s = state.sessions.find((s) => s.id === args.id);
    if (s) s.state = "stopped";
    return;
  }
  throw new Error("这是公开截图演示模式，此操作不会修改本机配置。");
}
