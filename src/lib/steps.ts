/**
 * 五个进度步骤的 id。
 *
 * ⚠ 这五个 id 与 Rust 侧 `qb_platform::progress::STEP_IDS` 硬绑定，
 * 也是 `%LOCALAPPDATA%\ClaudeIpGate\progress.json` 里**已有数据的 key**。
 * 改名 = 老用户进度全丢。加新 id 也要两边一起改。
 *
 * 侧栏早就不是「①②③④⑤ 向导」，新手引导那一页也删了（2026-09-27）。
 * 现在真正被读的只有 `purity`（综合评分里纯净度的人工判定，见 `score.ts`）；
 * 其余几步照常记、照常读得进来，只是界面上不再按步骤展示。
 * `accounts` 原来只有新手引导写，现在没人写了 —— id 留着，老数据里有它。
 *
 * 「中文环境识别」拆成了独立页面，但它**不是第六个步骤** —— 它归在
 * `environment` 这一步里，由软件页负责记录。这样 progress.json 的形状不变。
 */

export type StepId = "purity" | "environment" | "dns" | "iplock" | "accounts";
