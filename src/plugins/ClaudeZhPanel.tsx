/**
 * Claude 桌面端 · 中文界面（插件 claude-desktop-zh-cn）的详情面板。
 *
 * 内容就是账户页「汉化」弹窗里那一个组件（`features/zh/ClaudeZh`）——
 * 同一块功能只写一份，两处打开看到的永远是同一个状态、同一套按钮。
 */
import ClaudeZh from "../features/zh/ClaudeZh";

export default function ClaudeZhPanel() {
  return (
    <div className="mt-3">
      <ClaudeZh />
    </div>
  );
}
