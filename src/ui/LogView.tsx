import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Copy } from "lucide-react";
import Button from "./Button";

interface Props {
  lines: string[];
  /** 自动滚到底。任务跑起来的时候要开，静态日志不用。 */
  follow?: boolean;
  empty?: ReactNode;
  actions?: ReactNode;
  /** 命中这些片段的行标红。 */
  errorHints?: string[];
}

const DEFAULT_ERROR_HINTS = [
  "错误",
  "失败",
  "error",
  "Error",
  "ERROR",
  "failed",
  "FAIL",
];

/**
 * 日志窗。
 *
 * 替掉 `.logbox`（`arr.join('\n')` 塞进一个死高度 150px 的 div）：
 * 那个既不会自动滚到底，也没法复制，出问题时用户只能看见最早的几行。
 */
export default function LogView({
  lines,
  follow = false,
  empty = "（暂无记录）",
  actions,
  errorHints = DEFAULT_ERROR_HINTS,
}: Props) {
  const ref = useRef<HTMLDivElement>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!follow) return;
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines, follow]);

  async function copy() {
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      /* 剪贴板不给用就算了 */
    }
  }

  return (
    <div>
      <div className="mb-1.5 flex items-center gap-1.5">
        <span className="notice">{lines.length} 行</span>
        <span className="ml-auto flex items-center gap-1.5">
          {actions}
          <Button
            size="sm"
            variant="ghost"
            onClick={copy}
            disabled={lines.length === 0}
            icon={
              copied ? (
                <Check size={12} aria-hidden="true" />
              ) : (
                <Copy size={12} aria-hidden="true" />
              )
            }
          >
            {copied ? "已复制" : "复制全部"}
          </Button>
        </span>
      </div>
      <div className="logview" ref={ref} role="log" aria-live="polite">
        {lines.length === 0
          ? empty
          : lines.map((l, i) => (
              <div
                key={i}
                className={
                  errorHints.some((h) => l.includes(h))
                    ? "logview-line--error"
                    : undefined
                }
              >
                {l}
              </div>
            ))}
      </div>
    </div>
  );
}
