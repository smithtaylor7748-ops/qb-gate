# 验证记录

执行日期：2026-09-10。测试对象为此独立包；未修改或构建正在开发的 ClaudeGate 主仓库。

## 已通过

- `npm run check`：TypeScript 5.6.3 strict、noUnusedLocals、noUnusedParameters 检查通过。
- `npm test`：27 个来源 ID 唯一；4 条路线各 6 步；全部步骤、FAQ 的来源引用存在；外链拒绝非 HTTPS、伪装域名、URL 凭证和异常端口。
- `npm run build`：生成单文件离线预览、完整带来源 Markdown 和 React / React DOM / Scheduler 许可声明。
- Microsoft Edge 152.0.4191.66 + Playwright：从本地 file:// 成功打开；Claude Pro / ChatGPT Plus × Android / iPhone 共 24 步的上下步、勾选、完成提示通过。
- 不同服务与渠道的进度彼此独立，切回后仍保留本次页面的进度；刷新后重置，未存入 localStorage。
- 视频页 2 个参考条目、来源关键词筛选与空结果提示、FAQ 与手机环境展开、原生键盘操作通过。
- 1440、1280、1024、820、768、390、360 像素宽度下，四个内容分区均没有整页横向溢出。
- 深色、浅色、手机和视频页面均已截图并人工查看；没有遮挡主要正文或控件。
- 浏览器 pageerror / console error 为 0；页面加载和内部交互触发的自动 HTTP(S) 请求为 0。

浏览器检测结果见 `preview/browser-check.json`。界面图见同目录 PNG；这些图是本组件的真实渲染，不是 Apple、Google 或 AI App 的真实支付截图。

## 实现检查

React 状态由用户事件更新；图文数据与呈现分开；使用原生 button、checkbox、details 和可见焦点。无任意 HTML 注入、动态脚本、外部追踪或远程字体。组件的 CSS 均位于 `.cg-guide` 作用域；演示外壳样式放在 demo/，不供宿主导入。无需新增主项目 Rust 命令或改变安全检查进度。

## 未测试与限制

未登录支付宝、Apple、Google、Claude 或 ChatGPT；没有真实充值、绑卡、续费、退款或购买测试。没有逐段播放 B 站视频。未验证每条外链在所有地区、设备和时间均可访问。未在最终 Tauri 应用中验证 opener 权限和宿主导航，实际合并后需按 README 的接入检查处理。

没有进行完整 WCAG 审计或所有浏览器兼容性测试。浏览器检查验证的是独立页面的功能，不证明网络、支付和服务账户一定可用，也不构成主项目的完整法律或安全审计。
