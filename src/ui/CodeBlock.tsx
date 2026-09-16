import { useState, type ReactNode } from "react";
import { Check, Copy } from "lucide-react";
import Button from "./Button";

interface Props {
  text: string;
  /** 左上角的说明，比如提示词标题。 */
  caption?: ReactNode;
  showLineNumbers?: boolean;
  actions?: ReactNode;
  maxHeight?: number;
}

/**
 * 只读代码 / 提示词展示。
 *
 * 替掉三处 `<textarea readOnly rows={14/16/18}>` —— 那玩意儿可聚焦、可选中、
 * 行数写死，长文只能在一个小窗口里滚，而且**三个页面三种复制反馈**：
 * DnsLeak 有「已复制」2 秒，Environment 的 PromptBlock 自己又实现一遍，
 * Settings 干脆点了没反应。这里统一成一个。
 */
export default function CodeBlock({
  text,
  caption,
  showLineNumbers = true,
  actions,
  maxHeight,
}: Props) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // 剪贴板不给用时不报错 —— 文本就在眼前，用户可以自己选中复制。
    }
  }

  const lines = text.split("\n");

  return (
    <div className="codeblock">
      <div className="codeblock-head">
        <span className="min-w-0 truncate">{caption}</span>
        <span className="ml-auto flex flex-shrink-0 items-center gap-1.5">
          {actions}
          <Button
            size="sm"
            variant="ghost"
            onClick={copy}
            icon={
              copied ? (
                <Check size={12} aria-hidden="true" />
              ) : (
                <Copy size={12} aria-hidden="true" />
              )
            }
          >
            {copied ? "已复制" : "复制"}
          </Button>
        </span>
      </div>
      <pre
        className="codeblock-body"
        style={maxHeight ? { maxHeight } : undefined}
      >
        {lines.map((line, i) => (
          <div className="codeblock-line" key={i}>
            {showLineNumbers && (
              <span className="codeblock-no" aria-hidden="true">
                {i + 1}
              </span>
            )}
            <span>{line || " "}</span>
          </div>
        ))}
      </pre>
    </div>
  );
}
