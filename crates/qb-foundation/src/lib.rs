// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
//! QB Gate 的地基层（L0）。
//!
//! # 这一层装什么
//!
//! 只装**所有人都要、而它谁都不要**的东西：
//!
//! | 模块 | 回答的问题 |
//! |---|---|
//! | [`error`] | 出错了怎么表达 |
//! | [`paths`] | 本机数据放哪 |
//! | [`audit`] | 使用者能看的那份日志怎么写 |
//! | [`sink`] | 领域代码怎么在**不认识 UI 框架**的前提下往界面报进度 |
//!
//! # 为什么它得是一个单独的 crate
//!
//! 体检时这三样东西分别住在 `gate` 里、`relay` 里、没人写。其中
//! `gate::state_dir` 与 `gate::log::write` 两个函数，撑起了 `gate` 入度 25
//! 里的绝大部分 —— 12 个跟门禁毫无关系的模块只为了问这两句就 `use crate::gate`，
//! 然后 `gate` 又反过来调它们，于是 15 对循环依赖里 9 对缠着门禁。
//!
//! 先搬成模块（A0），再拆成 crate（W0）。拆成 crate 多出来的那一点是
//! **Cargo 层面的不可能**：这个 crate 的依赖清单里没有本项目的任何别的 crate，
//! 所以它在物理上长不出反向依赖。模块边界靠测试守，crate 边界靠编译器守。

pub mod audit;
pub mod error;
pub mod paths;
pub mod sink;
