# 0.22.0 账户与检测回归（2026-09-16）

## 交付行为

- 官方账户导航内，Claude / Codex 在右侧纵向切换。
- 左侧账户与右侧启动、用量整体等宽等高。四个槽位一页；页面固定，滚轮不移动页面。统计说明、配置检查与环境待办在弹窗中处理。
- Codex 指 Microsoft Store 桌面端。新槽位交由官方窗口完成 ChatGPT 登录；切换和启动先明确确认关闭旧桌面窗口。默认账户目录保持原处。
- 五项检测逐项报告证据与错误。可修复的浏览器用户策略和符合条件的执行锁支持批量修复、随后复检；浏览器策略保留原值以便撤销。

## 自动检查

- `npm run build`：通过。
- `npm test`：8 个文件、79 项通过。
- `npm run types:check`：143 个导出类型名称唯一，前后端合同一致。
- `npm run format:check`：通过。
- `npm run test:ui`：76 个响应式/主题用例及 9 个交互流程通过。
- 固定布局另检查 1320×940、1180×820、900×740、680×640 的 Claude / Codex 两侧：列宽与高度相等、四个账户可见、无内容溢出或文字纵向裁切、滚轮不滚动主页面。另比较滚轮前后静态区域截图，防止 Edge 在 DOM 滚动值为零时仍出现视口弹性回弹。
- `npm run release:check`：通过。
- `cargo test --workspace --quiet`：869 项通过，含 11 项架构检查。
- `cargo clippy --workspace --lib -- -D warnings`、`cargo fmt --all -- --check`、`cargo deny check`：通过。

截图：`screenshots/accounts-layout-light.png`、`screenshots/codex-accounts-light.png`、`screenshots/checkup-repair-light.png`。截图使用演示数据。

## 本机交付

- `npm run tauri build` 成功生成 `QB Gate_0.22.0_x64-setup.exe`，已静默更新本机并重新打开 QB Gate。
- 安装版本为 0.22.0，窗口响应正常，桌面快捷方式指向安装目录。
- 安装后的可执行文件与构建产物仅有 Tauri 的包类型标记差异（`UNK` → `NSS`）；按 NSIS 标记核算后 SHA-256 一致。
- 安装包 SHA-256：`84FC7445C36D9D9C620A29706A1362B3A70FCCCC51564E120A11C84428F9498C`。
- 旧安装可执行文件与快捷方式保存在本机忽略目录 `target/before-account-redesign/`，未提交运行期资料。

## 验证边界

自动测试使用临时目录或演示数据，没有终止当前 Codex、完成真实 OAuth 登录、修改真实注册表策略或执行 ACL 修复。官方桌面端的独立目录参数已在本机安装版本中核对；真实登录和账户切换仍需在用户保存任务后操作。桌面端仅使用其独立槽位的本机会话统计，不把默认目录历史或订阅剩余额度冒充成本槽位用量。
