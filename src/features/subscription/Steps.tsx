import type { ReactNode } from "react";
import { Pill } from "../../ui";
import type { Tone } from "../../ui";

/**
 * 编号步骤条。`<ol>` 是真的有序列表：读屏器能报「第 3 步，共 6 步」，
 * 编号由 CSS 计数器画出来，不用手写数字。
 */
export function StepList({ children }: { children: ReactNode }) {
  return <ol className="qb-sub-steps">{children}</ol>;
}

interface StepProps {
  title: ReactNode;
  /** 右侧的小标签，比如「首选」「认准开发者」。 */
  tag?: { tone: Tone; text: string };
  children: ReactNode;
  /** 步骤下面那条浅色提示。 */
  tip?: ReactNode;
  /** 红色提示（地址那类），比 tip 更显眼；两个可以同时给。 */
  warn?: ReactNode;
}

export function Step({ title, tag, children, tip, warn }: StepProps) {
  return (
    <li className="qb-sub-step">
      <div className="qb-sub-step-body">
        <h3 className="qb-sub-step-title">
          <span>{title}</span>
          {tag && <Pill tone={tag.tone}>{tag.text}</Pill>}
        </h3>
        <div className="qb-sub-step-text">{children}</div>
        {tip && <div className="qb-sub-tip">{tip}</div>}
        {warn && <div className="qb-sub-tip qb-sub-tip--danger">{warn}</div>}
      </div>
    </li>
  );
}
