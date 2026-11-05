/**
 * 会话管理器（中转站重构阶段 6 · S1）的 IPC 包装。类型全部来自 `generated/`。
 * 后端是 `usecase::session_ops`：只读本机文件，删除先挪进面板的回收站。
 */
import { call } from "./ipc";
import type { AntigravityConversation } from "./generated/AntigravityConversation";
import type { SessionDetail } from "./generated/SessionDetail";
import type { SessionList } from "./generated/SessionList";
import type { TrashEntry } from "./generated/TrashEntry";

export type { AntigravityConversation, SessionDetail, SessionList, TrashEntry };
export type { SessionSummary } from "./generated/SessionSummary";
export type { SessionMessage } from "./generated/SessionMessage";

export const sessionsApi = {
  /**
   * 列表：`text` 搜标题 / 项目目录 / 会话 id；`content` 读文件搜内容（有上限）；
   * `kind` = `chat` / `subagent` / `null`（全部）。
   */
  list: (q: {
    text: string;
    content: string;
    kind: string | null;
    source: string | null;
    offset: number;
    limit: number;
  }) => call<SessionList>("sessions_list", q),
  read: (id: string, offset: number, limit: number) =>
    call<SessionDetail>("sessions_read", { id, offset, limit }),
  /** 写成 Markdown 到使用者在保存框里选的路径（只认 `.md`）。 */
  exportTo: (id: string, path: string) =>
    call<void>("sessions_export", { id, path }),
  /** 在资源管理器里选中这个会话文件。 */
  reveal: (id: string) => call<void>("sessions_reveal", { id }),
  /** 挪进面板的回收站（两分钟内还在写的会被拒绝）。 */
  remove: (id: string) => call<TrashEntry>("sessions_delete", { id }),
  trash: () => call<TrashEntry[]>("sessions_trash"),
  restore: (id: string) => call<TrashEntry>("sessions_trash_restore", { id }),
  /** `id: null` = 清空会话回收站。 */
  purge: (id: string | null) => call<number>("sessions_trash_purge", { id }),
  /** 反重力的对话（全机共用，不分账户）：只读列表。 */
  antigravity: () =>
    call<AntigravityConversation[]>("antigravity_conversations"),
};
