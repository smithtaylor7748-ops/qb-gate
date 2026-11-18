/**
 * 就地确认：点了先在原位展开一句后果 + ［确认］［取消］，再点一次才执行（2026-11-15，A2 / A5）。
 *
 * 不弹第二层窗口：这些动作多半已经在弹窗里（备用地址、写进其他工具、会话归桶），
 * 叠一层弹窗要按两次 Esc 才回得去。后果写在按钮旁边，不放悬停提示。
 */

import { useState, type ReactNode } from "react";

/** 一句后果 + 两个按钮。行内、弹窗里都能放。 */
export function InlineConfirm({
  message,
  confirmLabel,
  onConfirm,
  onCancel,
  danger = false,
  busy = false,
}: {
  message: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  danger?: boolean;
  busy?: boolean;
}) {
  return (
    <span
      className={`qb-st-confirm${danger ? " danger" : ""}`}
      role="group"
      aria-label="确认操作"
    >
      <span className="msg">{message}</span>
      <span className="qb-st-inline">
        <button
          type="button"
          className={`qb-st-iconbtn${danger ? " qb-st-iconbtn--danger" : " qb-st-iconbtn--go"}`}
          disabled={busy}
          onClick={onConfirm}
        >
          {confirmLabel}
        </button>
        <button
          type="button"
          className="qb-st-iconbtn"
          disabled={busy}
          onClick={onCancel}
        >
          取消
        </button>
      </span>
    </span>
  );
}

/**
 * 一个小按钮，点了先就地问一句。`label` 是平时的按钮字，`message` 是问的那一句。
 *
 * 只给一点就会改东西、又撤不回来（或撤回很麻烦）的动作用：删备用地址、从其他工具里删、
 * 断开账单登录、还原会话备份。
 */
export function ConfirmButton({
  label,
  ariaLabel,
  title,
  message,
  confirmLabel,
  onConfirm,
  disabled = false,
  danger = true,
}: {
  label: ReactNode;
  ariaLabel?: string;
  title?: string;
  message: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  disabled?: boolean;
  danger?: boolean;
}) {
  const [asking, setAsking] = useState(false);
  if (asking)
    return (
      <InlineConfirm
        message={message}
        confirmLabel={confirmLabel}
        danger={danger}
        busy={disabled}
        onCancel={() => setAsking(false)}
        onConfirm={() => {
          setAsking(false);
          onConfirm();
        }}
      />
    );
  return (
    <button
      type="button"
      className="qb-st-iconbtn"
      aria-label={ariaLabel}
      title={title}
      disabled={disabled}
      onClick={() => setAsking(true)}
    >
      {label}
    </button>
  );
}
