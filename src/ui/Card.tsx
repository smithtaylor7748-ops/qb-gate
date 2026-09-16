import type { ReactNode } from "react";
import type { Tone } from "./labels";

interface Props {
  /** 标题走 `<h2>`，卡片内部**不再出现标题标签** —— 旧代码卡中卡加 h2，
   *  读屏器读出来的层级是乱的。 */
  title?: ReactNode;
  icon?: ReactNode;
  /** 右上角的操作区。 */
  actions?: ReactNode;
  tone?: Tone;
  children?: ReactNode;
  className?: string;
  /** 标题渲染成哪一级。默认 h2；嵌在别的区块里时给 h3。 */
  as?: "h2" | "h3";
}

export default function Card({
  title,
  icon,
  actions,
  tone = "default",
  children,
  className = "",
  as: Heading = "h2",
}: Props) {
  const cls = ["card", tone !== "default" && `card--${tone}`, className]
    .filter(Boolean)
    .join(" ");

  return (
    <section className={cls}>
      {(title || actions) && (
        <div className="card-head">
          {title && (
            <Heading className="card-title">
              {icon}
              {title}
            </Heading>
          )}
          {actions && <div className="card-actions">{actions}</div>}
        </div>
      )}
      {children}
    </section>
  );
}
