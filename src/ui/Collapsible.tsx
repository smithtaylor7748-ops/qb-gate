import { useId, useState, type ReactNode } from "react";
import { ChevronRight } from "lucide-react";

interface Props {
  /** 触发行的文字。长说明统一用「为什么这么做？」这类问句。 */
  summary: ReactNode;
  children: ReactNode;
  defaultOpen?: boolean;
  className?: string;
}

/**
 * 折叠块。
 *
 * 替掉三处 `<div onClick>` + `▾` / `▸` 字符：那种写法键盘走不到、
 * 读屏器不知道它能展开、也不知道当前是开是关。这里是真的 `<button>`
 * 带 `aria-expanded` 与 `aria-controls`。
 */
export default function Collapsible({
  summary,
  children,
  defaultOpen = false,
  className = "",
}: Props) {
  const [open, setOpen] = useState(defaultOpen);
  const bodyId = useId();

  return (
    <div className={`collapsible ${className}`}>
      <button
        type="button"
        className="collapsible-trigger"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => setOpen((v) => !v)}
      >
        <ChevronRight
          size={13}
          className="collapsible-chevron"
          aria-hidden="true"
        />
        {summary}
      </button>
      <div id={bodyId} className="collapsible-body" hidden={!open}>
        {children}
      </div>
    </div>
  );
}
