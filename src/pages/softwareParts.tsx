/**
 * 软件页几张卡共用的两块版式。原来是 `Environment.tsx` 里的局部函数；2026-09-29 软件页多了
 * SillyTavern（酒馆）那张卡（`features/tavern/SillyTavernCard.tsx`），它要用同样的栏与进度块 ——
 * 挪到这里导出，免得那张卡反过来 import `Environment.tsx`（两边互相引用就是一个环）。
 */
import type { ReactNode } from "react";

import type { TaskState } from "../lib/tasks";
import { LogView, ProgressBar } from "../ui";

/**
 * 长任务的进度条 + 日志。
 *
 * ⛔ **要画在按下去的那张卡里。** 0.28.0 之前 `install` 这个任务的进度块只渲染在
 * 第一张卡（Claude Code），而 Codex CLI 与 Claude 桌面端的「安装」按钮走的是同一个
 * 任务名 —— 点的是第三张卡，进度条出现在页面最上面那张卡里，按钮那一带一动不动。
 * 所以共用任务名的几张卡要用 `show` 指明「这一次是谁在跑」。
 */
export function TaskBlock({
  task,
  show = true,
  label = "安装进度",
}: {
  task: TaskState;
  show?: boolean;
  label?: string;
}) {
  if (!show || (!task.running && task.log.length === 0)) return null;
  return (
    <div className="mt-3">
      <ProgressBar
        value={task.total > 0 ? (task.step / task.total) * 100 : undefined}
        tone={task.error ? "danger" : "accent"}
        label={label}
      />
      <p className="notice mt-1">{task.phase}</p>
      <LogView lines={task.log} follow={task.running} />
    </div>
  );
}

/** 卡内一栏。三栏之间用左边框分隔，窄屏时自然堆叠。 */
export function Col({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <section className="min-w-0 border-line px-1 py-2 lg:border-l lg:px-4 lg:first:border-l-0 lg:first:pl-0">
      <span className="notice mb-2 block font-mono uppercase tracking-wide">
        {label}
      </span>
      {children}
    </section>
  );
}
