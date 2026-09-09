import type { ReactNode } from 'react';

interface Props {
  icon?: ReactNode;
  title: string;
  /** 一句话说清楚为什么是空的。 */
  children?: ReactNode;
  /** 主行动。**空状态必须给出路** —— 旧代码的账户页说「先登录一次即可建立」
   *  却不给任何按钮，是一条死路。 */
  action?: ReactNode;
}

export default function EmptyState({ icon, title, children, action }: Props) {
  return (
    <div className="emptystate">
      {icon && <span className="emptystate-icon">{icon}</span>}
      <span className="emptystate-title">{title}</span>
      {children && <p className="emptystate-body">{children}</p>}
      {action && <div className="mt-1 flex gap-2">{action}</div>}
    </div>
  );
}
