import type { ReactNode } from 'react';

interface Props {
  title: ReactNode;
  /** 一句话说清这一页是干什么的。长说明请折叠，不要堆在这里。 */
  sub?: ReactNode;
  actions?: ReactNode;
}

export default function PageHeader({ title, sub, actions }: Props) {
  return (
    <header className="mb-4 flex items-start gap-3">
      <div className="min-w-0 flex-1">
        <h1>{title}</h1>
        {sub && <p className="sub">{sub}</p>}
      </div>
      {actions && <div className="flex flex-shrink-0 items-center gap-2">{actions}</div>}
    </header>
  );
}
