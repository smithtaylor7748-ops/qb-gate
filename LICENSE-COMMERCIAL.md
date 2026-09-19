# 商业授权说明（Commercial License）

QB Gate 采用**双授权**：

| | 授权 | 适用 |
|---|---|---|
| 默认 | **AGPL-3.0-only**（见 [LICENSE](LICENSE)） | 任何人，免费 |
| 可选 | **商业授权**（本文件） | 不能或不愿履行 AGPL 义务的场景 |

两者是二选一。除非你另行取得商业授权，否则你拿到的这份 QB Gate 就是 AGPL-3.0-only 的。

**注意是 `-only`，不是 `-or-later`。** [README 的授权声明](README.md#授权声明)里
**故意没有**写「或任何更新版本」，所以 FSF 将来若发布 AGPL 新版本，**不会**自动适用于
本程序（依据 AGPL 第 14 节）。版权人是 **smithtaylor7748-ops**。

---

## 一、AGPL 这一档要你做什么

拿免费这一档，你要守 AGPL-3.0 的全部条款，其中最容易被忽略的两条：

1. **分发即开源。** 你把 QB Gate（原样或改过的）交给别人 —— 装机、打包、内部下发都算 ——
   就要把对应的完整源码一并给到，且同样按 AGPL-3.0-only 授权。
2. **联网提供服务也算（§13）。** 这是 AGPL 与 GPL 的区别所在：
   哪怕你不分发二进制，只是把改过的 QB Gate 架起来**通过网络给别人用**，
   也必须向这些使用者提供你那份修改后的完整源码。

还有：保留版权与许可声明、标注你做过的修改、不得追加与 AGPL 冲突的限制；以及
[LICENSE-ADDITIONAL-TERMS.md](LICENSE-ADDITIONAL-TERMS.md) 里依 AGPL 第 7 节附加的三条
（保留署名与仓库链接、改版必须标明、不授予「QB Gate」名称）。
完整条款以 [LICENSE](LICENSE) 英文原文为准，本节只是提示，不构成法律意见。

## 二、什么时候需要商业授权

下列任一情形，AGPL 这一档走不通，需要单独谈商业授权：

- 把 QB Gate 的代码**并入闭源软件**，或随闭源产品一起分发，而不打算开源该软件；
- 以 QB Gate 为基础**对外提供网络服务／SaaS／托管服务**，而不打算按 §13 公开你的修改；
- 需要**去掉 AGPL 的 copyleft 传染性**，或需要与 AGPL 不相容的其他许可证并存；
- 需要合同层面的**担保、赔偿、技术支持或责任承担** —— AGPL 明确不提供这些
  （见 LICENSE 第 15、16 节与 [DISCLAIMER.md](DISCLAIMER.md)）。

反过来说：**自己用、内部用、改完只给自己用，不分发也不对外提供网络服务 ——
AGPL 就够了，不需要来买授权。**

## 三、商业授权覆盖什么、不覆盖什么

**覆盖：** QB Gate 自有代码的版权许可。版权人是 **smithtaylor7748-ops**
（`Copyright (C) 2026 smithtaylor7748-ops`，声明见 [README](README.md#授权声明)）——
所有提交均由其一人持有，因此有权在 AGPL 之外另行授权，商业授权也由其授出。

**不覆盖：**

- **第三方依赖与并入的第三方代码**，各自保留原许可证。
  清单见 [docs/dependencies.json](docs/dependencies.json)、
  [THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) 与 [ATTRIBUTION.md](ATTRIBUTION.md)。
  其中并入的 MIT 代码允许再许可，但**原版权声明必须随之保留**；
  MPL-2.0 组件按文件级 copyleft，对应文件的源码仍须可取得。
- **外部应用。** SillyTavern 等由使用者自行安装的 AGPL-3.0 应用是独立程序，
  本项目不捆绑、不链接、不分发，其许可义务不因商业授权而转移。
- **合规责任。** 商业授权只解决版权许可，不改变 [DISCLAIMER.md](DISCLAIMER.md)
  里的任何一条：本项目仍是非官方项目，不对账户状态作任何承诺，
  使用者仍须遵守所在地法律与各服务商条款。

## 四、贡献者条款

为了让双授权继续成立，向本项目提交代码即表示你同意：
你对所提交内容拥有授权的权利，并授予项目维护者在 **AGPL-3.0-only
与商业授权两种方式下使用、修改和再许可**该内容的权利。
不接受你无权授权的第三方代码 —— 来源与许可记录规则见 [CONTRIBUTING.md](CONTRIBUTING.md)。

## 五、怎么谈

商业授权按实际用途逐案商定，请说明：使用方式（内嵌 / 分发 / 对外服务）、
规模、是否需要支持与担保。

- GitHub Issue：https://github.com/smithtaylor7748-ops/qb-gate/issues
- QQ 群：`1109462206`

> 本文件是**授权说明**，不是授权合同本身；具体条款以双方签署的书面协议为准。
> 无论哪一档，涉及金额或合规风险的决定请自行咨询律师 —— 这份说明和本项目的
> 其他文档一样，不构成法律意见。

---

# Commercial License (English)

QB Gate is **dual-licensed**:

- **AGPL-3.0-only** (see [LICENSE](LICENSE)) — free, for everyone.
- **Commercial license** — for cases where the AGPL's obligations cannot be met.

Unless you have separately obtained a commercial license, the copy of QB Gate you
received is licensed to you under AGPL-3.0-only, supplemented by the section 7
additional terms in [LICENSE-ADDITIONAL-TERMS.md](LICENSE-ADDITIONAL-TERMS.md)
(attribution, marking of modified versions, no grant of the "QB Gate" name).

You need a commercial license if you want to link QB Gate into closed-source
software, distribute it as part of a proprietary product, or offer a modified
version as a network service without publishing your modifications under AGPL §13.
You do **not** need one for private or internal use where nothing is distributed
and no network service is offered to others.

Note the licence is AGPL-3.0-**only**: the notice in [README](README.md#授权声明)
deliberately omits "or any later version", so versions of the AGPL that the FSF may
publish in future do not apply to this program automatically (AGPL section 14).

A commercial license covers copyright in QB Gate's own code only, and is granted by
the copyright holder, **smithtaylor7748-ops**. Third-party
dependencies and incorporated third-party code keep their own licenses — see
[docs/dependencies.json](docs/dependencies.json),
[THIRD_PARTY_NOTICES.txt](THIRD_PARTY_NOTICES.txt) and [ATTRIBUTION.md](ATTRIBUTION.md).
It does not alter [DISCLAIMER.md](DISCLAIMER.md): this remains an unofficial
project that makes no promises about account status.

By contributing, you agree that you have the right to license what you submit and
that you grant the maintainers the right to use, modify and sublicense it under
**both** AGPL-3.0-only and a commercial license.

To enquire: https://github.com/smithtaylor7748-ops/qb-gate/issues

> This file describes the offer; it is not the contract, and it is not legal advice.
