# 0.22.3 中转站修复与验证

本轮修复范围：重复标题和常驻提示条、站点保存/凭证绑定、转发与检验数据源、Codex 桌面启动、启动/关闭切换。

## 已确认的根因

1. RouteDialog 的 Key 只用于临时探测，保存线路时没有调用 credential_save；编辑已有站点又直接复用 ID，未写回地址。
2. 新版界面写 SQLite providers/credentials，station 命令却从旧 relay.json 组装上游、模型、检验和调度。
3. Codex 配置由客户端写入 projects 后，整文件哈希改变便被拒绝；Claude 桌面配置文件也不在允许的清单内。
4. 本机路由会重复拼接 /v1；无客户端前缀的 Codex models 请求被判为 Claude Code。
5. 中转启动仍解析 CLI 路径。安装的 Windows 桌面运行时还有 Chromium 单实例判断，仅设置 CODEX_ELECTRON_USER_DATA_PATH 不足以选择独立资料目录，必须同时传 --user-data-dir。
6. 按钮只播放两秒成功动效，没有订阅真实会话状态。

## 验证口径

- 前端凭证测试验证：输入 Key 被保存并绑定；空输入保留凭证；跨站凭证/无效凭证被拒；保存失败不能报告成功。
- Rust 回归验证：按线路解析数据库 Key、配置合并保留用户字段、OAuth 隔离、三客户端配置哈希清单、URL 与客户端前缀、现有流式转发及关停规则。
- UI 回归新增：删除标题/提示条；启动后变关闭；切换软件不串状态；关闭后恢复启动。
- 手工桌面烟测命令：`cargo run -p qb-app --example relay-desktop-smoke`。只使用临时目录、虚构 Key 和回环模拟上游；核对 models/responses 路径与鉴权替换，打开真实 Store 桌面应用，45 秒后仅关闭创建的 Job 进程树，并确认原桌面进程仍在。
- 已通过桌面烟测；通过隔离窗口的 Chromium 调试接口看到桌面欢迎页，临时 CODEX_HOME 创建了自己的状态库，未进入 CLI。
- 本机旧线路没有保存的凭证，不能据此声称真实第三方模型调用或计费检验已经通过。更新后必须重新填 Key，再验证真实站点。

## 本机维护

D 盘构建曾因磁盘满失败。将本仓库 target/debug/incremental 迁至 E 盘的 qb-gate-build-cache/incremental-20260916 保留；后续构建设置 CARGO_INCREMENTAL=0。未移动源码、用户配置或应用数据。

## 自动检查结果

- `npm run build`、`npm test`：106 项测试通过。
- `npm run test:ui`：76 个响应式/主题组合与 10 个交互流程通过，其中包含启动/关闭切换。
- `cargo test --workspace`：892 项测试通过，含结构约束测试。
- `cargo clippy --workspace --lib -- -D warnings`、`cargo fmt --all -- --check`：通过。
- `npm run types:check`：146 个导出类型与 Rust 一致。
- `npm run format:check`、`npm run release:check`：通过。
- `cargo deny check`：advisories/bans/licenses/sources 全部通过；已有重复依赖提示为 warning。

## 本机安装核验

- 已用 NSIS 安装包更新本机安装目录，程序与卸载项版本均为 0.22.3；桌面快捷方式指向该安装目录。
- 安装后的 exe 与 release 产物仅有打包类型标记的 3 字节差别（`UNK` → `NSS`），其余内容一致。
- 在实际安装版中确认：重复标题和常驻提示条已删除；已有 Codex 线路仍在；点击启动明确报告缺少 API Key，没有误切换为关闭按钮。
- 检查后已退出临时调试实例，正常启动安装版；原有 Codex 桌面对话进程仍在。
