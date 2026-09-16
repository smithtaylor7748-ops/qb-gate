import { call } from "./ipc";
import { res } from "./store";
import type { CodexAccounts } from "./generated/CodexAccounts";
import type { CodexDesktop } from "./generated/CodexDesktop";
import type { CodexUsage } from "./generated/CodexUsage";

export const codexApi = {
  accounts: () => call<CodexAccounts>("codex_accounts"),
  desktop: () => call<CodexDesktop>("codex_desktop_status"),
  create: (label: string) => call<string>("codex_create", { label }),
  switch: (id: string) => call<void>("codex_switch", { id }),
  launch: (id: string) => call<void>("codex_launch", { id }),
  close: () => call<void>("codex_close"),
  archive: (id: string) => call<void>("codex_archive", { id }),
  usage: (id: string, days: number) =>
    call<CodexUsage>("codex_usage", { id, days }),
};
export const CODEX_R = {
  accounts: res(codexApi.accounts, { pollMs: 15_000 }),
  desktop: res(codexApi.desktop, { pollMs: 30_000 }),
};
