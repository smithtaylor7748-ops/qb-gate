import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink as Icon } from "lucide-react";
import type { ReactNode } from "react";

interface Props {
  href: string;
  children: ReactNode;
  /** 显示成按钮的样子（放在操作区里时）。 */
  asButton?: boolean;
  className?: string;
}

/**
 * 外链。
 *
 * 两个旧毛病一起修：
 *   - `<a href="#" onClick={...}>` 右键「复制链接地址」拿到的是 `#`，
 *     状态栏也看不出要去哪；
 *   - 有些外链干脆是 `<button>`，中键点不出新窗口，也不像链接。
 *
 * 这里 `href` 写真地址（右键复制、悬停预览都对），点击时拦下来交给
 * 系统浏览器打开 —— WebView 里直接导航会把面板自己顶掉。
 */
export default function ExternalLink({
  href,
  children,
  asButton = false,
  className = "",
}: Props) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noreferrer noopener"
      className={
        asButton
          ? `btn btn--md no-underline hover:no-underline ${className}`
          : `inline-flex items-center gap-1 ${className}`
      }
      onClick={(e) => {
        e.preventDefault();
        void openUrl(href);
      }}
    >
      {children}
      <Icon size={11} aria-hidden="true" />
      <span className="sr-only">（在系统浏览器中打开）</span>
    </a>
  );
}
