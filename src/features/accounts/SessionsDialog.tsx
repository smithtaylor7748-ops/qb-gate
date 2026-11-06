/**
 * 账户页（Claude / GPT / 反重力）的「会话」入口与弹窗。
 *
 * 账户页是固定高度的，会话列表放弹窗里：顶上选槽位（默认当前槽位），下面是锁到这个槽位的
 * `SessionsPanel`（分组、类型筛选、两种搜索、阅读 / 导出 / 恢复命令 / 回收站都在那里）。
 * 反重力的对话库全机共用、不分账户：只读列表（时间、大小、能读出的标题），不删。
 */
import { useCallback, useEffect, useState } from "react";
import { MessagesSquare } from "lucide-react";

import SessionsPanel, { bytes } from "../SessionsPanel";
import { sessionsApi, type AntigravityConversation } from "../../lib/sessions";
import type { SessionSourceCount } from "../../lib/generated/SessionSourceCount";
import { Button, EmptyState, Modal } from "../../ui";
import { ago } from "./sessionGroups";

export type SessionsKind = "claude" | "codex" | "antigravity";

const PREFIX: Record<Exclude<SessionsKind, "antigravity">, string> = {
  claude: "claude-code:slot:",
  codex: "codex:slot:",
};

/** 槽位选择 + 锁到这个槽位的会话列表。 */
function SlotSessions({
  kind,
  current,
}: {
  kind: Exclude<SessionsKind, "antigravity">;
  current?: string;
}) {
  const prefix = PREFIX[kind];
  const [picked, setPicked] = useState<string | null>(
    current ? prefix + current : null,
  );
  const [slots, setSlots] = useState<SessionSourceCount[]>([]);
  const onSources = useCallback(
    (all: SessionSourceCount[]) =>
      setSlots(all.filter((s) => s.id.startsWith(prefix))),
    [prefix],
  );
  // 没有「当前槽位」时，等列表回来再选第一个。
  useEffect(() => {
    if (!picked && slots.length) setPicked(slots[0].id);
  }, [picked, slots]);
  const value = picked ?? `${prefix}?`;
  const known = slots.some((s) => s.id === value);
  return (
    <SessionsPanel
      key={value}
      lockedSource={value}
      onSources={onSources}
      toolbarStart={
        <select
          className="qb-session-select"
          aria-label="槽位"
          title="看哪个槽位的会话"
          value={value}
          onChange={(e) => setPicked(e.target.value)}
        >
          {!known && (
            <option value={value}>
              {picked ? picked.slice(prefix.length) : "（还没有槽位）"}
            </option>
          )}
          {slots.map((s) => (
            <option key={s.id} value={s.id}>
              {s.label.replace(/^.*官方槽位\s*/, "")} · {s.count} 个
            </option>
          ))}
        </select>
      }
      hint={
        kind === "claude"
          ? "也列出官方默认目录里归这个账户的会话；找不到归属依据的单列一组。"
          : undefined
      }
    />
  );
}

/** 反重力：全机共用的对话库，只读。 */
function AntigravityList() {
  const [items, setItems] = useState<AntigravityConversation[] | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let live = true;
    sessionsApi
      .antigravity()
      .then((x) => live && setItems(x))
      .catch((e: unknown) => live && setError(String(e)));
    return () => {
      live = false;
    };
  }, []);
  return (
    <section
      className="qb-session-panel"
      data-testid="antigravity-conversations"
    >
      <p className="qb-session-hint">
        对话库在 <code>~\.gemini\antigravity*\conversations</code>
        ，全机共用、不分账户；这里只读，不删不改。
      </p>
      {error && <p className="notice notice--danger">{error}</p>}
      <div className="qb-session-list">
        {!items ? (
          !error && <p className="qb-session-loading">正在读对话库…</p>
        ) : items.length === 0 ? (
          <div className="qb-session-empty">
            <EmptyState
              icon={<MessagesSquare size={20} />}
              title="这台机器上还没有反重力的对话"
            />
          </div>
        ) : (
          items.map((c) => (
            <div key={`${c.dir}/${c.id}`} className="qb-session-item">
              <div className="qb-session-main">
                <span className="qb-session-trash-name" title={c.title ?? ""}>
                  {c.title ??
                    (c.legacy ? "旧格式归档，读不出标题" : "读不出标题")}
                </span>
                <span className="qb-session-meta2">
                  <span>{bytes(c.size)}</span>
                  <span>{c.dir}</span>
                  <code className="qb-session-code">{c.id}</code>
                </span>
              </div>
              <div className="qb-session-side">
                <span className="qb-session-when">{ago(c.updated_at)}</span>
              </div>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

export function SessionsDialog({
  kind,
  current,
  onClose,
}: {
  kind: SessionsKind;
  /** 当前槽位：Claude 是标签，Codex 是槽位 id。 */
  current?: string;
  onClose: () => void;
}) {
  const title =
    kind === "claude"
      ? "Claude 会话"
      : kind === "codex"
        ? "Codex 会话"
        : "反重力对话（全机共用，不分账户）";
  return (
    <Modal
      open
      onClose={onClose}
      size="huge"
      title={title}
      footer={<Button onClick={onClose}>关闭</Button>}
    >
      {kind === "antigravity" ? (
        <AntigravityList />
      ) : (
        <SlotSessions kind={kind} current={current} />
      )}
    </Modal>
  );
}

/** 账户列表标题行上的「会话」按钮（三个 Band 各放一个）。 */
export function SessionsEntry({
  kind,
  current,
}: {
  kind: SessionsKind;
  current?: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <>
      {/* 只画图标：账户页标题栏 680px 宽时放不下第二个带字的按钮（固定高度的页面会被挤裁）。 */}
      <Button
        size="sm"
        icon={<MessagesSquare size={12} />}
        aria-label="会话"
        title="会话"
        data-testid={`sessions-entry-${kind}`}
        onClick={() => setOpen(true)}
      />
      {open && (
        <SessionsDialog
          kind={kind}
          current={current}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}
