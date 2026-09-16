/**
 * 五个进度步骤。
 *
 * ⚠ 这五个 id 与 `src-tauri/src/progress.rs` 的 `STEP_IDS` 硬绑定，
 * 也是 `%LOCALAPPDATA%\ClaudeIpGate\progress.json` 里**已有数据的 key**。
 * 改名 = 老用户进度全丢。加新 id 也要两边一起改。
 *
 * 侧栏不再是「①②③④⑤ 向导」——它现在按功能分组，进度只用在总览顶部那条
 * 可关闭的横幅上。但进度本身照常记：哪一步没走过、哪一步是被知情跳过的，
 * 仍然要能查得到。
 *
 * 「中文环境识别」拆成了独立页面，但它**不是第六个步骤** —— 它归在
 * `environment` 这一步里，由环境页负责记录。这样 progress.json 的形状不变。
 */

export const STEPS = [
  { id: "purity", label: "IP 纯净度" },
  { id: "environment", label: "环境与安装" },
  { id: "dns", label: "DNS 泄露" },
  { id: "iplock", label: "IP 锁" },
  { id: "accounts", label: "账户与启动" },
] as const;

export type StepId = (typeof STEPS)[number]["id"];

export function stepLabel(id: string): string {
  return STEPS.find((s) => s.id === id)?.label ?? id;
}
