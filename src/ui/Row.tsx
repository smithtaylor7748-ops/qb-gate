import type { ReactNode } from "react";

interface RowProps {
  /** 左侧内容。 */
  children: ReactNode;
  /** 右侧操作或状态。**没有右栏的场景请用 `<Bullet>`**。 */
  side?: ReactNode;
  className?: string;
}

/**
 * 两栏行：左内容、右操作。
 *
 * 旧代码把 `.row`（`space-between` 的两栏 flex）拿去渲染 DNS findings 的单条
 * 红字、「合规边界」的四条编号列表 —— 那些没有右栏，用两栏容器只是借它的
 * 下边框，结果是文字莫名其妙贴在左边、编号和内容之间撑开一大片空白。
 */
export function Row({ children, side, className = "" }: RowProps) {
  return (
    <div className={`row ${className}`}>
      <span className="row-main">{children}</span>
      {side && <span className="row-side">{side}</span>}
    </div>
  );
}

interface BulletProps {
  /** 编号或符号。不给就是一个圆点。 */
  marker?: ReactNode;
  tone?: "default" | "warn" | "danger";
  children: ReactNode;
}

/** 单条列表项：编号说明、findings 红字这类，没有右栏。 */
export function Bullet({ marker, tone = "default", children }: BulletProps) {
  return (
    <div className={`bullet${tone !== "default" ? ` bullet--${tone}` : ""}`}>
      <span className="bullet-marker" aria-hidden="true">
        {marker ?? "·"}
      </span>
      <span className="min-w-0">{children}</span>
    </div>
  );
}
