import { call } from "./ipc";
import type { SlotCredential } from "./generated/SlotCredential";
import type { OfficialRefreshOutcome } from "./generated/OfficialRefreshOutcome";

export type CredentialProvider = "claude" | "gpt" | "antigravity";

/** 账户行上的凭证状态（只读本机，不联网）与「让官方换新」。 */
export const credentialApi = {
  status: (provider: CredentialProvider) =>
    call<SlotCredential[]>("account_credentials", { provider }),
  /** 「邮箱对不上」那条提醒：使用者确认过，就以现在的邮箱为准。 */
  emailAccept: (provider: CredentialProvider, id: string) =>
    call<void>("account_email_accept", { provider, id }),
  /**
   * GPT 访问令牌到点：后台起官方 `codex app-server`，让它自己换新。
   * 面板不读不存令牌；这个槽位的桌面端 / 桥接在跑就跳过并说明。
   */
  codexOfficialRefresh: (id: string) =>
    call<OfficialRefreshOutcome>("codex_official_refresh", { id }),
};
