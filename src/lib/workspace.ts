import { CHANNELS } from "./channels";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { useToast } from "../ui";
import { DEMO_ENABLED } from "./demo";
import { invalidate, invalidateAutomatic, res, useResource } from "./store";
import { call } from "./ipc";
import { toIpcError } from "./ipcError";
import type { Workspace } from "./generated/Workspace";
import type { Provider } from "./generated/Provider";
import type { Credential } from "./generated/Credential";
import type { Environment } from "./generated/Environment";
import type { McpServer } from "./generated/McpServer";
import type { McpView } from "./generated/McpView";
import type { McpImportReport } from "./generated/McpImportReport";
import type { SkillsView } from "./generated/SkillsView";
import type { SkillCandidate } from "./generated/SkillCandidate";
import type { SkillUpdate } from "./generated/SkillUpdate";
import type { SkillsShHit } from "./generated/SkillsShHit";
import type { Session } from "./generated/Session";
import type { Client } from "./generated/Client";
import type { IdentityKind } from "./generated/IdentityKind";
import type { ExtensionManifest } from "./generated/ExtensionManifest";
import type { ExtensionInstallation } from "./generated/ExtensionInstallation";
import type { InstallRequest } from "./generated/InstallRequest";
export type {
  Workspace,
  Provider,
  Credential,
  Environment,
  Session,
  Client,
  IdentityKind,
  ExtensionManifest,
  ExtensionInstallation,
  InstallRequest,
};

// `call` 在 `ipc.ts` 里 —— 跟 `api.ts` 用的是同一个（B2）。
export type { ConfigPreview } from "./generated/ConfigPreview";
export type { InstallPreview } from "./generated/InstallPreview";
import type { ConfigPreview } from "./generated/ConfigPreview";
import type { InstallPreview } from "./generated/InstallPreview";
/** `blocked` 非空 = 有恢复记录结不清，恢复页要给出放弃它们的入口。 */
export interface StartupStatus {
  ready: boolean;
  error: string | null;
  blocked: string[];
}
/**
 * ⛔ 这里只放界面真的在调的命令（2026-09-27 清过一遍）。
 *
 * 删掉的那一批都是「服务商与环境」旧页和新手引导在用的：环境的预览 / 应用 / 回滚、
 * 导出导入、引用检查与删除、草稿诊断，外加从来没有界面的启动方案、取消操作、
 * 放弃会话记录。后端的命令一起删了 —— `src-tauri/tests/architecture.rs` 的
 * `every_registered_command_is_called_by_the_frontend` 盯着两边对得上。
 */
export const workspaceApi = {
  startup: () => call<StartupStatus>("startup_status"),
  retryStartup: () => call<StartupStatus>("startup_retry"),
  discardBlockedRecovery: () => call<StartupStatus>("startup_discard_blocked"),
  state: () => call<Workspace>("workspace_state"),
  saveProvider: (provider: Provider) =>
    call<Provider>("provider_save", { provider }),
  saveCredential: (credential: Credential, action: string, key?: string) =>
    call<Credential>("credential_save", { credential, action, key }),
  officialPreview: (client: Client, id: string) =>
    call<ConfigPreview[]>("official_configuration_preview", { client, id }),
  officialMigrate: (client: Client, id: string, fingerprint: string) =>
    call<string>("official_configuration_migrate", { client, id, fingerprint }),
  launch: (
    client: Client,
    kind: IdentityKind,
    id: string,
    workingDir?: string,
  ) => call<Session>("session_launch", { client, kind, id, workingDir }),
  stop: (id: string) => call<void>("session_stop", { id }),
  catalog: () => call<ExtensionManifest[]>("extension_catalog"),
  // MCP 面板（阶段 5，M1）。原来的「导入 MCP 进目录」撤了，MCP 都在这里管。
  mcpList: () => call<McpView>("mcp_list"),
  mcpSave: (server: McpServer, previousId: string | null) =>
    call<McpView>("mcp_save", { server, previousId }),
  mcpDelete: (id: string) => call<McpView>("mcp_delete", { id }),
  mcpSetTarget: (id: string, target: string, enabled: boolean) =>
    call<McpView>("mcp_set_target", { id, target, enabled }),
  mcpImport: (target: string) =>
    call<McpImportReport>("mcp_import", { target }),
  // Skills 面板（阶段 5，M2）。原来的「导入 Skills 进目录」撤了，Skills 都在这里管。
  skillsList: () => call<SkillsView>("skills_list"),
  skillsDiscover: (source: string) =>
    call<SkillCandidate[]>("skills_discover", { source }),
  skillsDiscoverZip: (path: string) =>
    call<SkillCandidate[]>("skills_discover_zip", { path }),
  skillsInstall: (picked: SkillCandidate[], targets: string[]) =>
    call<SkillsView>("skills_install", { picked, targets }),
  skillsInstallZip: (path: string, names: string[], targets: string[]) =>
    call<SkillsView>("skills_install_zip", { path, names, targets }),
  skillsSetTarget: (id: string, target: string, enabled: boolean) =>
    call<SkillsView>("skills_set_target", { id, target, enabled }),
  skillsUninstall: (id: string) => call<SkillsView>("skills_uninstall", { id }),
  skillsRestore: (backup: string) =>
    call<SkillsView>("skills_restore", { backup }),
  skillsImport: (target: string, directory: string) =>
    call<SkillsView>("skills_import", { target, directory }),
  skillsCheckUpdates: () => call<SkillUpdate[]>("skills_check_updates"),
  skillsUpdate: (id: string) => call<SkillsView>("skills_update", { id }),
  skillsSearch: (query: string) =>
    call<SkillsShHit[]>("skills_search", { query }),
  previewExtension: (request: InstallRequest) =>
    call<InstallPreview>("extension_preview", { request }),
  install: (request: InstallRequest) =>
    call<ExtensionInstallation>("extension_install", { request }),
  uninstall: (id: string) => call<void>("extension_uninstall", { id }),
  // 原来这里有 `connectApplication`（扩展中心酒馆页的「接入已配置的本地安装」）。
  // 2026-09-29 酒馆变成面板的内置功能，那颗按钮连同命令一起删了。
  checkUpdates: () => call<string[]>("extension_check_updates"),
  checkMcp: (request: InstallRequest) =>
    call<{ connected: boolean; tools: string[]; detail: string }>(
      "extension_check",
      { request },
    ),
};
const definition = res(workspaceApi.state, { staleMs: 10_000 });
const catalog = res(workspaceApi.catalog);
export const useWorkspace = () => useResource("workspace", definition);
export const useCatalog = () => useResource("catalog", catalog);

export function useWorkspaceEvents() {
  useEffect(() => {
    if (DEMO_ENABLED) return;
    let disposed = false;
    const pending = listen<{ revision: number }>(
      CHANNELS.workspace_changed,
      () => void invalidateAutomatic(),
    );
    void pending
      .then((stop) => {
        if (disposed) stop();
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      void pending.then((stop) => stop()).catch(() => undefined);
    };
  }, []);
}
export function useAction() {
  const toast = useToast();
  const pendingRef = useRef(false);
  const [pending, setPending] = useState("");
  const [error, setError] = useState("");
  /**
   * 跑一个动作。成功之后**只刷新活状态**（`auto` 的那些资源），不清测量。
   *
   * ⛔ 原来这里是 `invalidateAll()`（2026-09-25 查出）：`auto: false` 的测量 —— DNS、中文环境、
   * 本机体检、出口一致性、出口 IP —— 连同存在 localStorage 里的那份一起扔掉。中转站存一条线路、
   * 设置里点一个开关、「检查官方目录」看一眼差异，总览上的综合评分都退回「未检测」，
   * 六秒的 DNS 要重测；引导页「扫描」刚刷新完的结果也被当场扔掉。
   *
   * 真改了某个被测对象的动作（恢复系统时区、重装 Chrome……）把那几份测量的键写进 `stale`，
   * 只扔那几份 —— 跟 `store.ts` 里 `invalidate` 的规矩同一条：破坏性操作之后，那一次测量就是没了。
   */
  async function run<T>(
    name: string,
    action: () => Promise<T>,
    success?: string,
    stale: readonly string[] = [],
  ): Promise<T | undefined> {
    if (pendingRef.current) return;
    pendingRef.current = true;
    setPending(name);
    setError("");
    try {
      const result = await action();
      if (stale.length) invalidate(...stale);
      void invalidateAutomatic();
      if (success) toast.ok(success);
      return result;
    } catch (e) {
      const err = toIpcError(e);
      setError(err.message);
      toast.error(err.message);
      return undefined;
    } finally {
      pendingRef.current = false;
      setPending("");
    }
  }
  return { pending, error, run };
}
export const CLIENT_NAMES: Record<Client, string> = {
  "claude-code": "Claude Code",
  "claude-desktop": "Claude 桌面端",
  codex: "Codex",
  antigravity: "反重力",
  "antigravity-ide": "反重力 IDE",
  "gemini-cli": "Gemini CLI",
  "grok-build": "Grok Build",
};
export const KIND_NAMES = {
  application: "应用集成",
  mcp: "MCP",
  skill: "Skills",
};
export const STATE_NAMES: Record<string, string> = {
  cancelled: "已取消",
  missing: "文件缺失",
  connected: "已接入",
  unreadable: "配置不可读",
  saved: "待应用",
  applied: "已应用",
  external: "外部已修改",
  running: "运行中",
  exited: "已退出",
  stopped: "已停止",
  failed: "失败",
  completed: "完成",
  interrupted: "已中断",
  enabled: "已启用",
  installed: "已安装",
  unverified: "待核验",
};
