import { CHANNELS } from "./channels";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef, useState } from "react";
import { useToast } from "../ui";
import { DEMO_ENABLED } from "./demo";
import { invalidateAll, invalidateAutomatic, res, useResource } from "./store";
import { call } from "./ipc";
import { toIpcError } from "./ipcError";
import type { LaunchPlan } from "./generated/LaunchPlan";
import type { Workspace } from "./generated/Workspace";
import type { Provider } from "./generated/Provider";
import type { Credential } from "./generated/Credential";
import type { Environment } from "./generated/Environment";
import type { Session } from "./generated/Session";
import type { Client } from "./generated/Client";
import type { IdentityKind } from "./generated/IdentityKind";
import type { ExtensionManifest } from "./generated/ExtensionManifest";
import type { ExtensionInstallation } from "./generated/ExtensionInstallation";
import type { InstallRequest } from "./generated/InstallRequest";
import type { ProbeReport } from "./generated/ProbeReport";
import type { ProbeRequest } from "./generated/ProbeRequest";
export type {
  LaunchPlan,
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
  ProbeReport,
  ProbeRequest,
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
export const workspaceApi = {
  cancelOperation: (id: string) => call<void>("operation_cancel", { id }),
  startup: () => call<StartupStatus>("startup_status"),
  retryStartup: () => call<StartupStatus>("startup_retry"),
  discardBlockedRecovery: () => call<StartupStatus>("startup_discard_blocked"),
  savePlan: (plan: LaunchPlan) =>
    call<LaunchPlan>("launch_plan_save", { plan }),
  removePlan: (id: string) => call<void>("launch_plan_remove", { id }),
  state: () => call<Workspace>("workspace_state"),
  saveProvider: (provider: Provider) =>
    call<Provider>("provider_save", { provider }),
  saveCredential: (credential: Credential, action: string, key?: string) =>
    call<Credential>("credential_save", { credential, action, key }),
  saveEnvironment: (environment: Environment) =>
    call<Environment>("environment_save", { environment }),
  previewEnvironment: (id: string) =>
    call<ConfigPreview[]>("environment_preview", { id }),
  rollbackEnvironment: (id: string) =>
    call<Environment>("environment_rollback", { id }),
  exportRelays: () => call<unknown>("relay_export"),
  importRelays: (bundle: unknown) => call<number>("relay_import", { bundle }),
  officialPreview: (client: Client, id: string) =>
    call<ConfigPreview[]>("official_configuration_preview", { client, id }),
  officialMigrate: (client: Client, id: string, fingerprint: string) =>
    call<string>("official_configuration_migrate", { client, id, fingerprint }),
  applyEnvironment: (id: string, fingerprint: string) =>
    call<Environment>("environment_apply", { id, fingerprint }),
  references: (kind: string, id: string) =>
    call<string[]>("workspace_references", { kind, id }),
  remove: (kind: string, id: string) =>
    call<void>("workspace_remove", { kind, id }),
  launch: (
    client: Client,
    kind: IdentityKind,
    id: string,
    workingDir?: string,
  ) => call<Session>("session_launch", { client, kind, id, workingDir }),
  stop: (id: string) => call<void>("session_stop", { id }),
  forgetSession: (id: string) => call<void>("session_forget", { id }),
  diagnose: (request: ProbeRequest) =>
    call<ProbeReport>("diagnostic_run", { request }),
  diagnostics: () => call<ProbeReport[]>("diagnostic_history"),
  catalog: () => call<ExtensionManifest[]>("extension_catalog"),
  importSkills: (source: string) =>
    call<ExtensionManifest[]>("extension_import_skills", { source }),
  importMcp: (name: string, configuration: string) =>
    call<ExtensionManifest>("extension_import_mcp", { name, configuration }),
  previewExtension: (request: InstallRequest) =>
    call<InstallPreview>("extension_preview", { request }),
  install: (request: InstallRequest) =>
    call<ExtensionInstallation>("extension_install", { request }),
  uninstall: (id: string) => call<void>("extension_uninstall", { id }),
  connectApplication: () =>
    call<ExtensionInstallation>("extension_connect_application"),
  checkUpdates: () => call<string[]>("extension_check_updates"),
  checkMcp: (request: InstallRequest) =>
    call<{ connected: boolean; tools: string[]; detail: string }>(
      "extension_check",
      { request },
    ),
};
const definition = res(workspaceApi.state, { staleMs: 10_000 });
const catalog = res(workspaceApi.catalog);
const diagnostics = res(workspaceApi.diagnostics, { staleMs: 10_000 });
export const useWorkspace = () => useResource("workspace", definition);
export const useCatalog = () => useResource("catalog", catalog);
export const useDiagnostics = () => useResource("diagnostics", diagnostics);

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
  async function run<T>(
    name: string,
    action: () => Promise<T>,
    success?: string,
  ): Promise<T | undefined> {
    if (pendingRef.current) return;
    pendingRef.current = true;
    setPending(name);
    setError("");
    try {
      const result = await action();
      invalidateAll();
      if (success) toast.ok(success);
      return result;
    } catch (e) {
      const err = toIpcError(e);
      // 使用者自己点的取消不是故障 —— 弹一个红色的「操作已由用户取消」
      // 只会让人以为哪里坏了。这个分支看的是 `kind`，不是 message 里的中文
      // （文案会改，而改文案的人不知道这里靠它做判断）。
      if (err.cancelled) return undefined;
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
};
export const KIND_NAMES = {
  application: "应用集成",
  mcp: "MCP",
  skill: "Skills",
  template: "配置模板",
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
