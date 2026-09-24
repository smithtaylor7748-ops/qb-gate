/**
 * 启动磁贴。总览的 Claude 侧和 GPT 侧共用同一份。
 *
 * v0.7.0 从 `AccountBand.tsx` 里抽出来：总览分成 Claude / GPT 两个页签之后，
 * 两边都要用它。留在原地复制一份的话，两个磁贴的进度线、失败态、
 * 「代价常驻」这几件事迟早会走样，而它们恰恰是这个组件存在的理由。
 */

import type { ReactNode } from "react";

import type { TaskState } from "../../lib/tasks";

export interface TileProps {
  icon: ReactNode;
  name: string;
  /** 常驻说明。**不要放进 title 属性** —— 代价必须一直看得见，见档案 §7。 */
  note: ReactNode;
  tone?: "accent" | "warn";
  task: TaskState;
  disabled?: boolean;
  onClick: () => void;
  /**
   * 给 `test:ui` 用的稳定选择器。
   *
   * 磁贴的可访问名是 `name + note` 拼起来的，而 `note` 会随运行状态、
   * 任务进度、失败原因变 —— 用名字精确匹配的断言必然时红时绿。
   */
  testId?: string;
  /** 额外的 class（例如让「一键关闭」那格横跨两列）。 */
  className?: string;
  /**
   * 压在磁贴右上角的小按钮（例如酒馆那格的「桥接设置」）。
   *
   * ⛔ **不能直接放进磁贴里** —— 磁贴本身就是一个 `<button>`，`button` 套 `button`
   * 是非法 HTML，浏览器解析时会把内层那个拆到外面去，点了谁都说不准。
   * 所以给磁贴包一层定位容器，角标是它的**兄弟节点**，绝对定位压在右上角。
   */
  corner?: ReactNode;
}

/**
 * 进度不再挂在贴外：正在启动时贴底出现一条 2px 进度线，`phase` 顶掉那行说明；
 * 失败则整贴变红并**留住错误原文**（toast 会自己消失，错误不能只靠它）。
 */
export default function Tile({
  icon,
  name,
  note,
  tone,
  task,
  disabled,
  onClick,
  testId,
  className,
  corner,
}: TileProps) {
  const failed = !!task.error;
  const cls = [
    "tile",
    failed ? "tile--danger" : tone ? `tile--${tone}` : "",
    // 有角标时外层那个容器才是网格子项，`className`（`launchcol-wide` 之类）
    // 跟着它走 —— 留在磁贴身上，跨列就不生效了。
    corner ? "" : (className ?? ""),
  ]
    .filter(Boolean)
    .join(" ");
  const pct = task.total > 0 ? (task.step / task.total) * 100 : undefined;

  const tile = (
    <button
      type="button"
      className={cls}
      disabled={disabled}
      onClick={onClick}
      data-testid={testId}
    >
      <span className="tile-icon" aria-hidden="true">
        {icon}
      </span>
      <span className="tile-name">{name}</span>
      <span className="tile-note">
        {failed ? task.error : task.running ? task.phase || "启动中…" : note}
      </span>
      {(task.running || failed) && (
        <span
          className={[
            "tile-progress",
            failed ? "tile-progress--danger" : "",
            task.running && pct === undefined
              ? "tile-progress--indeterminate"
              : "",
          ]
            .filter(Boolean)
            .join(" ")}
          style={
            failed
              ? { width: "100%" }
              : pct !== undefined
                ? { width: `${pct}%` }
                : undefined
          }
        />
      )}
    </button>
  );

  if (!corner) return tile;
  return (
    <div className={["tile-wrap", className ?? ""].filter(Boolean).join(" ")}>
      {tile}
      <span className="tile-corner">{corner}</span>
    </div>
  );
}
